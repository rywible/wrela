use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use wrela_runtime::db::membership_set_voters;
use wrela_runtime::db::rpc::grpc::GrpcEdgeService;
use wrela_runtime::db::rpc::private_network::{PrivateRpcServer, start_private_rpc_server};
use wrela_runtime::db::types::DbError;
use wrela_runtime::db::{
    DbClientWritePathAggregate, DbCommitVisibilityStatus, DbHealthStatus, DbWriteStageAggregate,
    QuorumTransportMode, ReplicatedLogBackend, close_db, db_client_write_path_aggregate,
    db_commit_visibility_status, db_health_status, db_write_stage_aggregate, read_point,
    submit_put,
};

const WORKLOAD_SCHEMA_VERSION: u32 = 14;
const SUMMARY_SCHEMA_VERSION: u32 = 18;

#[derive(Debug, Clone, Copy)]
enum WorkloadKind {
    RawLeaderLocal,
    RawRoundRobinNodes,
    ValidatedWritePath,
}

impl WorkloadKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::RawLeaderLocal => "raw_write_leader_local",
            Self::RawRoundRobinNodes => "raw_write_round_robin_nodes",
            Self::ValidatedWritePath => "validated_write_path",
        }
    }
}

#[derive(Debug, Default, Clone)]
struct WorkerMetrics {
    attempts: u64,
    success: u64,
    failures: u64,
    retry_after: u64,
    retry_after_by_cause: BTreeMap<String, u64>,
    retry_after_messages: BTreeMap<String, u64>,
    latencies_us: Vec<u64>,
    slow_ops: Vec<SlowOpOutlier>,
}

#[derive(Debug, Serialize)]
struct WorkloadReport {
    schema_version: u32,
    name: String,
    concurrency: usize,
    duration_seconds: u64,
    payload_bytes: usize,
    attempts: u64,
    success: u64,
    failures: u64,
    retry_after_pct: f64,
    retry_after_by_cause: BTreeMap<String, u64>,
    retry_after_messages: BTreeMap<String, u64>,
    tps: f64,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    p999_ms: f64,
    stage_avg_queue_wait_ms: f64,
    stage_avg_lane_dequeue_to_complete_ms: f64,
    stage_avg_queue_to_complete_ms: f64,
    queue_saturation_pct: f64,
    stage_engine_lock_wait_pct: f64,
    stage_validate_route_pct: f64,
    stage_replicate_pct: f64,
    stage_wal_append_pct: f64,
    stage_wal_submit_wait_pct: f64,
    stage_wal_hol_wait_pct: f64,
    stage_wal_queue_wait_pct: f64,
    stage_wal_encode_pct: f64,
    stage_wal_fdatasync_pct: f64,
    stage_wal_mutex_wait_pct: f64,
    stage_apply_pct: f64,
    stage_raft_persist_pct: f64,
    stage_clock_persist_pct: f64,
    stage_other_pct: f64,
    stage_percentiles: StagePercentilesReport,
    client_write_path: ClientWritePathTelemetryReport,
    replication: ReplicationTelemetryReport,
    writer_lanes: WriterLaneTelemetryReport,
    apply_lanes: ApplyLaneTelemetryReport,
    lsm: LsmTelemetryReport,
    latency_histogram: LatencyHistogram,
    slow_op_threshold_ms: u64,
    slow_ops: Vec<SlowOpOutlier>,
    stage_outlier_threshold_ms: u64,
    stage_outliers: Vec<StageOutlier>,
    client_path_outlier_threshold_ms: u64,
    client_path_outliers: Vec<ClientPathOutlier>,
}

#[derive(Debug, Serialize, Default)]
struct Percentiles {
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    p999_ms: f64,
}

#[derive(Debug, Serialize, Default)]
struct StagePercentilesReport {
    queue_wait: Percentiles,
    lane_dequeue_to_complete: Percentiles,
    queue_to_complete: Percentiles,
    engine_lock_wait: Percentiles,
    validate_route: Percentiles,
    replicate: Percentiles,
    wal_append: Percentiles,
    wal_submit_wait: Percentiles,
    wal_hol_wait: Percentiles,
    wal_fsync: Percentiles,
    apply: Percentiles,
    raft_persist: Percentiles,
    clock_persist: Percentiles,
    total: Percentiles,
}

#[derive(Debug, Serialize, Default)]
struct ReplicationTelemetryReport {
    telemetry_sample_period_ms: u64,
    queue_depth: u64,
    queue_depth_peak: u64,
    batch_samples: u64,
    batch_ops_le_1: u64,
    batch_ops_le_4: u64,
    batch_ops_le_16: u64,
    batch_ops_le_64: u64,
    batch_ops_gt_64: u64,
    batch_bytes_le_1k: u64,
    batch_bytes_le_4k: u64,
    batch_bytes_le_16k: u64,
    batch_bytes_le_64k: u64,
    batch_bytes_gt_64k: u64,
    quorum_ack_count: u64,
    quorum_size: u64,
    quorum_replication_ms: f64,
    quorum_fsync_ms: f64,
    target_count: u64,
    contacted_count: u64,
    wave_count: u64,
    wave_avg_targets: u64,
    wave_max_targets: u64,
    successful_count: u64,
    failed_count: u64,
    cancelled_count: u64,
    contact_efficiency_bps: u64,
    target_efficiency_bps: u64,
    skipped_count: u64,
    aborted_in_flight_count: u64,
    contacted_ratio_pct: f64,
    skipped_ratio_pct: f64,
    simulation_commits: u64,
    rpc_max_in_flight: u64,
    rpc_in_flight: u64,
    rpc_available_permits: u64,
    rpc_backpressure_timeouts: u64,
    rpc_backpressure_closed: u64,
    real_quorum_evidence: bool,
    quorum_transport_mode: String,
    replicated_log_backend: String,
    replicated_log_shadow_payload_bytes: u64,
    replicated_log_shadow_wal_bytes: u64,
    replicated_log_shadow_overhead_bytes: u64,
    replica_ack_count: u64,
    replica_durable_ack_count: u64,
    replica_max_replication_ms: f64,
    replica_max_fsync_ms: f64,
    apply_backlog_depth: u64,
    apply_backlog_peak: u64,
    depth_timeline: Vec<ReplicationDepthTimelinePoint>,
    quorum_failure_token: Option<String>,
    quorum_failure_reason: Option<String>,
    failure_counters: BTreeMap<String, u64>,
}

#[derive(Debug, Serialize, Default)]
struct WriterLaneTelemetryReport {
    lane_count: u64,
    active_lane_count: u64,
    total_assigned_shards: u64,
    max_assigned_shards: u64,
    max_assigned_shard_share_pct: f64,
    max_queue_depth: u64,
    max_lane_retry_after_bps: u64,
    max_lane_saturation_bps: u64,
    max_enqueue_attempt_share_bps: u64,
    assignment_lookups: u64,
    assignment_hits: u64,
    assignment_misses: u64,
    assignment_hit_rate_bps: u64,
    max_lane_retry_after_pct: f64,
    max_lane_saturation_pct: f64,
    max_enqueue_attempt_share_pct: f64,
    assignment_hit_rate_pct: f64,
}

#[derive(Debug, Serialize, Default)]
struct ApplyLaneTelemetryReport {
    lane_count: u64,
    active_lane_count: u64,
    max_queue_depth: u64,
}

#[derive(Debug, Serialize, Default)]
struct LsmTelemetryReport {
    compaction_debt_bytes_estimate: u64,
    shadow_bytes_estimate: u64,
    live_bytes_estimate: u64,
    total_bytes_estimate: u64,
    version_count: u64,
    tombstone_count: u64,
}

#[derive(Debug, Serialize, Clone, Default)]
struct ReplicationDepthTimelinePoint {
    elapsed_ms: u64,
    queue_depth: u64,
    queue_depth_peak: u64,
    apply_backlog_depth: u64,
    apply_backlog_peak: u64,
}

#[derive(Debug, Serialize, Default)]
struct LatencyHistogram {
    le_1ms: u64,
    le_5ms: u64,
    le_10ms: u64,
    le_25ms: u64,
    le_50ms: u64,
    le_100ms: u64,
    le_500ms: u64,
    le_1s: u64,
    le_5s: u64,
    le_10s: u64,
    gt_10s: u64,
}

#[derive(Debug, Serialize, Clone, Default)]
struct SlowOpOutlier {
    worker_id: usize,
    sequence: u64,
    handle: i64,
    total_ms: f64,
    write_primary_ms: f64,
    write_secondary_ms: f64,
    verify_read_ms: f64,
}

#[derive(Debug, Clone, Copy, Default)]
struct WriteOpTiming {
    total_us: u64,
    write_primary_us: u64,
    write_secondary_us: u64,
    verify_read_us: u64,
}

#[derive(Debug, Serialize, Clone, Default)]
struct StageOutlier {
    total_ms: f64,
    queue_wait_ms: f64,
    lane_dequeue_to_complete_ms: f64,
    queue_to_complete_ms: f64,
    engine_lock_wait_ms: f64,
    validate_route_ms: f64,
    replicate_ms: f64,
    wal_submit_wait_ms: f64,
    wal_hol_wait_ms: f64,
    wal_fsync_ms: f64,
}

#[derive(Debug, Serialize, Default)]
struct ClientWritePathTelemetryReport {
    sample_count: u64,
    forwarded_count: u64,
    avg_total_us: f64,
    preflight_pct: f64,
    enqueue_wait_pct: f64,
    response_wait_pct: f64,
    remote_forward_pct: f64,
    other_pct: f64,
    preflight_percentiles: Percentiles,
    enqueue_wait_percentiles: Percentiles,
    response_wait_percentiles: Percentiles,
    remote_forward_percentiles: Percentiles,
    total_percentiles: Percentiles,
}

#[derive(Debug, Serialize, Clone, Default)]
struct ClientPathOutlier {
    total_ms: f64,
    preflight_ms: f64,
    enqueue_wait_ms: f64,
    response_wait_ms: f64,
    remote_forward_ms: f64,
    forwarded: bool,
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn quorum_transport_mode_str(mode: QuorumTransportMode) -> &'static str {
    match mode {
        QuorumTransportMode::PreferPrivateRpc => "prefer_private_rpc",
        QuorumTransportMode::RequirePrivateRpc => "require_private_rpc",
    }
}

fn replicated_log_backend_str(mode: ReplicatedLogBackend) -> &'static str {
    match mode {
        ReplicatedLogBackend::DualWal => "dual_wal",
        ReplicatedLogBackend::ShadowCanonical => "shadow_canonical",
        ReplicatedLogBackend::CanonicalOnly => "canonical_only",
    }
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn env_bool(name: &str, default: bool) -> bool {
    std::env::var(name)
        .ok()
        .map(|raw| raw.trim().to_ascii_lowercase())
        .and_then(|raw| match raw.as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        })
        .unwrap_or(default)
}

struct EnvOverrideGuard {
    previous: Vec<(String, Option<String>)>,
}

impl EnvOverrideGuard {
    fn apply(overrides: &[(&str, String)]) -> Self {
        let mut previous = Vec::with_capacity(overrides.len());
        for (key, value) in overrides {
            let key = (*key).to_string();
            previous.push((key.clone(), std::env::var(&key).ok()));
            // SAFETY: Overrides are applied during fixture setup before worker
            // threads are spawned by this harness.
            unsafe {
                std::env::set_var(&key, value);
            }
        }
        Self { previous }
    }
}

impl Drop for EnvOverrideGuard {
    fn drop(&mut self) {
        for (key, prior) in self.previous.iter().rev() {
            match prior {
                Some(value) => {
                    // SAFETY: Restores original environment values during
                    // fixture teardown.
                    unsafe {
                        std::env::set_var(key, value);
                    }
                }
                None => {
                    // SAFETY: Restores original unset state during teardown.
                    unsafe {
                        std::env::remove_var(key);
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
struct WorkloadFixture {
    tempdirs: Vec<tempfile::TempDir>,
    handles: Vec<i64>,
    workload_handles: Vec<i64>,
    follower_servers: Vec<PrivateRpcServer>,
}

impl WorkloadFixture {
    fn teardown(mut self) {
        for server in &mut self.follower_servers {
            server.shutdown();
        }
        for handle in self.handles.drain(..) {
            assert!(close_db(handle), "close db handle {handle}");
        }
        drop(self.tempdirs);
    }
}

fn reserve_local_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("reserve local tcp port")
        .local_addr()
        .expect("local addr")
        .port()
}

fn open_real_quorum_fixture(kind: WorkloadKind) -> WorkloadFixture {
    use wrela_runtime::db::config::{DbConfig, EngineConfig, ReplicationConfig, TopologyConfig};
    use wrela_runtime::db::open_db_with_config;

    let mut tempdirs = Vec::new();
    let mut handles = Vec::new();

    // Followers: identity env vars only (no behavioral config via env).
    let follower_env_guard = EnvOverrideGuard::apply(&[
        ("WRELADB_PRIVATE_RPC_ENABLED", "0".to_string()),
        ("WRELADB_NODE_ID", "".to_string()),
        ("WRELADB_CLUSTER_NODES", "".to_string()),
        ("WRELADB_LEADER_NODE_ID", "".to_string()),
        ("WRELADB_PRIVATE_RPC_ADDRESS_MAP", "".to_string()),
        ("WRELADB_PRIVATE_RPC_BIND", "".to_string()),
    ]);

    let follower_config = DbConfig::for_testing();

    let follower_a_dir = tempfile::tempdir().expect("follower a tempdir");
    let follower_a_data = follower_a_dir
        .path()
        .join(format!("{}-node-2", kind.as_str()));
    fs::create_dir_all(&follower_a_data).expect("create follower a data dir");
    let follower_a_handle =
        open_db_with_config(&follower_a_data, &follower_config).expect("open follower a");

    let follower_b_dir = tempfile::tempdir().expect("follower b tempdir");
    let follower_b_data = follower_b_dir
        .path()
        .join(format!("{}-node-3", kind.as_str()));
    fs::create_dir_all(&follower_b_data).expect("create follower b data dir");
    let follower_b_handle =
        open_db_with_config(&follower_b_data, &follower_config).expect("open follower b");
    drop(follower_env_guard);

    let mut follower_servers = Vec::new();
    for (node_id, handle) in [("node2", follower_a_handle), ("node3", follower_b_handle)] {
        let mut service = GrpcEdgeService::new(node_id, "node1");
        service.bind_handle(handle);
        let service = Arc::new(RwLock::new(service));
        let server = start_private_rpc_server("127.0.0.1:0", service, Duration::from_millis(1500))
            .expect("start follower private rpc server");
        follower_servers.push(server);
    }

    let follower_a_addr = follower_servers[0].listen_addr().to_string();
    let follower_b_addr = follower_servers[1].listen_addr().to_string();
    let leader_port = reserve_local_port();
    let leader_addr = format!("127.0.0.1:{leader_port}");
    let address_map =
        format!("node1={leader_addr},node2={follower_a_addr},node3={follower_b_addr}");

    let leader_dir = tempfile::tempdir().expect("leader tempdir");
    let leader_data = leader_dir.path().join(format!("{}-node-1", kind.as_str()));
    fs::create_dir_all(&leader_data).expect("create leader data dir");
    // Leader: identity env vars for private mesh, typed config for transport mode.
    let env_guard = EnvOverrideGuard::apply(&[
        ("WRELADB_PRIVATE_RPC_ENABLED", "1".to_string()),
        ("WRELADB_NODE_ID", "node1".to_string()),
        ("WRELADB_CLUSTER_NODES", "node1,node2,node3".to_string()),
        ("WRELADB_LEADER_NODE_ID", "node1".to_string()),
        ("WRELADB_PRIVATE_RPC_PORT", leader_port.to_string()),
        ("WRELADB_PRIVATE_RPC_BIND", leader_addr),
        ("WRELADB_PRIVATE_RPC_ADDRESS_MAP", address_map),
        ("WRELADB_PRIVATE_RPC_MIN_READY_NODES", "1".to_string()),
    ]);
    let perf_lane_count = env_usize("WRELA_LOCAL_PERF_WRITER_LANE_COUNT", 1);
    let perf_logical_shards = env_usize("WRELA_LOCAL_PERF_LOGICAL_SHARDS", 1) as u32;
    let leader_config = DbConfig::for_testing()
        .with_replication(ReplicationConfig {
            quorum_transport_mode: QuorumTransportMode::RequirePrivateRpc,
            ..Default::default()
        })
        .with_engine(EngineConfig {
            writer_lane_count: perf_lane_count.max(1),
            ..EngineConfig::default()
        })
        .with_topology(TopologyConfig {
            initial_logical_shards: perf_logical_shards.max(1),
            ..TopologyConfig::default()
        });
    let leader_handle =
        open_db_with_config(&leader_data, &leader_config).expect("open leader with private mesh");
    drop(env_guard);

    membership_set_voters(leader_handle, vec![1, 2, 3]).expect("leader membership voters");

    tempdirs.push(leader_dir);
    tempdirs.push(follower_a_dir);
    tempdirs.push(follower_b_dir);
    handles.push(leader_handle);
    handles.push(follower_a_handle);
    handles.push(follower_b_handle);

    WorkloadFixture {
        tempdirs,
        handles,
        workload_handles: vec![leader_handle],
        follower_servers,
    }
}

fn percentile_ms(mut values_us: Vec<u64>, percentile: f64) -> f64 {
    if values_us.is_empty() {
        return 0.0;
    }
    values_us.sort_unstable();
    let idx = ((values_us.len() - 1) as f64 * percentile).round() as usize;
    values_us[idx] as f64 / 1_000.0
}

fn percentile_ms_ref(values_us: &[u64], percentile: f64) -> f64 {
    percentile_ms(values_us.to_vec(), percentile)
}

fn percentiles_from_us(values_us: &[u64]) -> Percentiles {
    Percentiles {
        p50_ms: percentile_ms_ref(values_us, 0.50),
        p95_ms: percentile_ms_ref(values_us, 0.95),
        p99_ms: percentile_ms_ref(values_us, 0.99),
        p999_ms: percentile_ms_ref(values_us, 0.999),
    }
}

fn average_u64(values: &[u64]) -> u64 {
    if values.is_empty() {
        return 0;
    }
    (values.iter().copied().sum::<u64>() / values.len() as u64).max(0)
}

fn us_to_ms(us: u64) -> f64 {
    us as f64 / 1_000.0
}

fn ns_to_ms(ns: u64) -> f64 {
    ns as f64 / 1_000_000.0
}

fn host_name() -> Option<String> {
    std::env::var("HOSTNAME")
        .ok()
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
}

fn run_metadata_json() -> Value {
    json!({
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "cpu_count": std::thread::available_parallelism().ok().map(|value| value.get()),
        "hostname": host_name(),
    })
}

fn sample_replication_depth(
    handles: &[i64],
    started: Instant,
) -> Option<ReplicationDepthTimelinePoint> {
    let health: Vec<DbHealthStatus> = handles
        .iter()
        .filter_map(|handle| db_health_status(*handle).ok())
        .collect();
    let visibility: Vec<DbCommitVisibilityStatus> = handles
        .iter()
        .filter_map(|handle| db_commit_visibility_status(*handle).ok())
        .collect();
    if health.is_empty() && visibility.is_empty() {
        return None;
    }
    let queue_depth_values = health
        .iter()
        .map(|item| item.replication_queue_depth)
        .collect::<Vec<_>>();
    let queue_depth_peak_values = health
        .iter()
        .map(|item| item.replication_queue_depth_peak)
        .collect::<Vec<_>>();
    let apply_backlog_values = visibility
        .iter()
        .map(|item| item.apply_backlog_depth)
        .collect::<Vec<_>>();
    let apply_backlog_peak_values = health
        .iter()
        .map(|item| item.apply_backlog_peak)
        .collect::<Vec<_>>();
    Some(ReplicationDepthTimelinePoint {
        elapsed_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
        queue_depth: average_u64(&queue_depth_values),
        queue_depth_peak: queue_depth_peak_values.into_iter().max().unwrap_or(0),
        apply_backlog_depth: average_u64(&apply_backlog_values),
        apply_backlog_peak: apply_backlog_peak_values.into_iter().max().unwrap_or(0),
    })
}

fn push_top_slow_op(outliers: &mut Vec<SlowOpOutlier>, item: SlowOpOutlier, cap: usize) {
    outliers.push(item);
    outliers.sort_by(|a, b| b.total_ms.total_cmp(&a.total_ms));
    outliers.truncate(cap.max(1));
}

fn push_top_stage_outlier(outliers: &mut Vec<StageOutlier>, item: StageOutlier, cap: usize) {
    outliers.push(item);
    outliers.sort_by(|a, b| b.total_ms.total_cmp(&a.total_ms));
    outliers.truncate(cap.max(1));
}

fn push_top_client_path_outlier(
    outliers: &mut Vec<ClientPathOutlier>,
    item: ClientPathOutlier,
    cap: usize,
) {
    outliers.push(item);
    outliers.sort_by(|a, b| b.total_ms.total_cmp(&a.total_ms));
    outliers.truncate(cap.max(1));
}

fn latency_histogram(values_us: &[u64]) -> LatencyHistogram {
    let mut hist = LatencyHistogram::default();
    for &value in values_us {
        let ms = value / 1_000;
        if ms <= 1 {
            hist.le_1ms = hist.le_1ms.saturating_add(1);
        } else if ms <= 5 {
            hist.le_5ms = hist.le_5ms.saturating_add(1);
        } else if ms <= 10 {
            hist.le_10ms = hist.le_10ms.saturating_add(1);
        } else if ms <= 25 {
            hist.le_25ms = hist.le_25ms.saturating_add(1);
        } else if ms <= 50 {
            hist.le_50ms = hist.le_50ms.saturating_add(1);
        } else if ms <= 100 {
            hist.le_100ms = hist.le_100ms.saturating_add(1);
        } else if ms <= 500 {
            hist.le_500ms = hist.le_500ms.saturating_add(1);
        } else if ms <= 1_000 {
            hist.le_1s = hist.le_1s.saturating_add(1);
        } else if ms <= 5_000 {
            hist.le_5s = hist.le_5s.saturating_add(1);
        } else if ms <= 10_000 {
            hist.le_10s = hist.le_10s.saturating_add(1);
        } else {
            hist.gt_10s = hist.gt_10s.saturating_add(1);
        }
    }
    hist
}

fn weighted_client_path_metric(
    aggregates: &[DbClientWritePathAggregate],
    selector: fn(&DbClientWritePathAggregate) -> f64,
) -> f64 {
    let total_samples: u64 = aggregates.iter().map(|item| item.sample_count).sum();
    if total_samples == 0 {
        return 0.0;
    }
    aggregates
        .iter()
        .map(|item| selector(item) * (item.sample_count as f64))
        .sum::<f64>()
        / (total_samples as f64)
}

fn workload_handles_for_metrics(kind: WorkloadKind, handles: &[i64]) -> Vec<i64> {
    match kind {
        WorkloadKind::RawRoundRobinNodes => handles.to_vec(),
        _ => handles.first().copied().into_iter().collect(),
    }
}

fn is_retry_after(err: &DbError) -> bool {
    err.message.contains("RETRY_AFTER_MS")
}

fn retry_after_cause(err: &DbError) -> Option<String> {
    if !is_retry_after(err) {
        return None;
    }
    let token = err
        .message
        .split(':')
        .next()
        .map(str::trim)
        .unwrap_or_default();
    let token_like = !token.is_empty()
        && token
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch == '_' || ch.is_ascii_digit());
    if token_like {
        return Some(token.to_string());
    }
    if err.message.contains("private mesh not ready") {
        return Some("PRIVATE_MESH_NOT_READY".to_string());
    }
    if err.message.contains("detached writer queue saturated") {
        return Some("WRITER_QUEUE_SATURATED".to_string());
    }
    Some("RETRY_AFTER_UNCLASSIFIED".to_string())
}

fn retry_after_message_key(err: &DbError) -> String {
    if !is_retry_after(err) {
        return "NON_RETRY_AFTER".to_string();
    }
    let base = err
        .message
        .split("; RETRY_AFTER_MS=")
        .next()
        .unwrap_or(err.message.as_str())
        .trim()
        .replace('\n', " ");
    let max_chars = 220usize;
    if base.chars().count() <= max_chars {
        return base;
    }
    let mut truncated = String::new();
    for ch in base.chars().take(max_chars) {
        truncated.push(ch);
    }
    truncated.push_str("...");
    truncated
}

fn top_count_map(source: BTreeMap<String, u64>, limit: usize) -> BTreeMap<String, u64> {
    let mut entries = source.into_iter().collect::<Vec<_>>();
    entries.sort_by(|(ak, av), (bk, bv)| bv.cmp(av).then_with(|| ak.cmp(bk)));
    entries.truncate(limit.max(1));
    entries.into_iter().collect()
}

fn run_write_op(
    kind: WorkloadKind,
    handle: i64,
    worker_id: usize,
    sequence: u64,
    payload: &[u8],
) -> Result<WriteOpTiming, DbError> {
    let total_started = Instant::now();
    match kind {
        WorkloadKind::RawLeaderLocal | WorkloadKind::RawRoundRobinNodes => {
            let key = format!("perf-raw-{worker_id}-{sequence}").into_bytes();
            let write_started = Instant::now();
            submit_put(handle, b"perf".to_vec(), key, payload.to_vec(), None)?;
            let write_primary_us = write_started.elapsed().as_micros().min(u64::MAX as u128) as u64;
            let total_us = total_started.elapsed().as_micros().min(u64::MAX as u128) as u64;
            Ok(WriteOpTiming {
                total_us,
                write_primary_us,
                write_secondary_us: 0,
                verify_read_us: 0,
            })
        }
        WorkloadKind::ValidatedWritePath => {
            let key = format!("perf-valid-{worker_id}-{sequence}").into_bytes();
            let value = payload.to_vec();
            let write_primary_started = Instant::now();
            submit_put(handle, b"perf".to_vec(), key.clone(), value.clone(), None)?;
            let write_primary_us = write_primary_started
                .elapsed()
                .as_micros()
                .min(u64::MAX as u128) as u64;
            let write_secondary_started = Instant::now();
            submit_put(
                handle,
                b"perf_meta".to_vec(),
                b"last_key".to_vec(),
                key.clone(),
                None,
            )?;
            let write_secondary_us = write_secondary_started
                .elapsed()
                .as_micros()
                .min(u64::MAX as u128) as u64;
            let read_started = Instant::now();
            let observed = read_point(handle, b"perf".to_vec(), key)?
                .ok_or_else(|| DbError::io("validated path read-after-write missing value"))?;
            let verify_read_us = read_started.elapsed().as_micros().min(u64::MAX as u128) as u64;
            if observed != value {
                return Err(DbError::io(
                    "validated path read-after-write mismatch in perf harness",
                ));
            }
            let total_us = total_started.elapsed().as_micros().min(u64::MAX as u128) as u64;
            Ok(WriteOpTiming {
                total_us,
                write_primary_us,
                write_secondary_us,
                verify_read_us,
            })
        }
    }
}

fn workload_handles(kind: WorkloadKind, handles: &[i64], worker_id: usize, sequence: u64) -> i64 {
    match kind {
        WorkloadKind::RawLeaderLocal | WorkloadKind::ValidatedWritePath => handles[0],
        WorkloadKind::RawRoundRobinNodes => {
            let idx = (worker_id + (sequence as usize)) % handles.len().max(1);
            handles[idx]
        }
    }
}

fn run_workload(
    kind: WorkloadKind,
    handles: &[i64],
    concurrency: usize,
    duration: Duration,
    payload_bytes: usize,
) -> WorkloadReport {
    let slow_op_threshold_ms = env_u64("WRELA_LOCAL_PERF_SLOW_OP_THRESHOLD_MS", 100);
    let max_slow_ops = env_usize("WRELA_LOCAL_PERF_MAX_SLOW_OPS", 32);
    let stage_outlier_threshold_ms = env_u64("WRELA_LOCAL_PERF_STAGE_OUTLIER_THRESHOLD_MS", 25);
    let max_stage_outliers = env_usize("WRELA_LOCAL_PERF_MAX_STAGE_OUTLIERS", 32);
    let client_path_outlier_threshold_ms =
        env_u64("WRELA_LOCAL_PERF_CLIENT_PATH_OUTLIER_THRESHOLD_MS", 25);
    let max_client_path_outliers = env_usize("WRELA_LOCAL_PERF_MAX_CLIENT_PATH_OUTLIERS", 32);
    let telemetry_sample_period_ms = env_u64("WRELA_LOCAL_PERF_TELEMETRY_SAMPLE_MS", 250).max(1);
    let barrier = Arc::new(Barrier::new(concurrency.max(1)));
    let results: Arc<Mutex<Vec<WorkerMetrics>>> = Arc::new(Mutex::new(Vec::new()));
    let payload = vec![b'x'; payload_bytes];
    let start = Instant::now();
    let metric_handles = workload_handles_for_metrics(kind, handles);
    let timeline_samples: Arc<Mutex<Vec<ReplicationDepthTimelinePoint>>> =
        Arc::new(Mutex::new(Vec::new()));
    let timeline_active = Arc::new(AtomicBool::new(true));
    let telemetry_sampler = {
        let sampler_handles = metric_handles.clone();
        let sampler_start = start;
        let sampler_samples = timeline_samples.clone();
        let sampler_active = timeline_active.clone();
        thread::spawn(move || {
            let sample_period = Duration::from_millis(telemetry_sample_period_ms);
            while sampler_active.load(Ordering::Relaxed) {
                if let Some(sample) = sample_replication_depth(&sampler_handles, sampler_start) {
                    if let Ok(mut shared) = sampler_samples.lock() {
                        shared.push(sample);
                    }
                }
                thread::sleep(sample_period);
            }
            if let Some(sample) = sample_replication_depth(&sampler_handles, sampler_start) {
                if let Ok(mut shared) = sampler_samples.lock() {
                    let should_push = shared
                        .last()
                        .map(|last| last.elapsed_ms != sample.elapsed_ms)
                        .unwrap_or(true);
                    if should_push {
                        shared.push(sample);
                    }
                }
            }
        })
    };
    let mut workers = Vec::with_capacity(concurrency);

    for worker_id in 0..concurrency {
        let worker_barrier = barrier.clone();
        let worker_results = results.clone();
        let worker_handles = handles.to_vec();
        let worker_payload = payload.clone();
        let worker_slow_threshold_us = slow_op_threshold_ms.saturating_mul(1_000);
        let worker_max_slow_ops = max_slow_ops;
        workers.push(thread::spawn(move || {
            let mut metrics = WorkerMetrics::default();
            worker_barrier.wait();
            let deadline = Instant::now() + duration;
            let mut sequence = 0u64;
            while Instant::now() < deadline {
                sequence = sequence.saturating_add(1);
                metrics.attempts = metrics.attempts.saturating_add(1);
                let handle = workload_handles(kind, &worker_handles, worker_id, sequence);
                match run_write_op(kind, handle, worker_id, sequence, &worker_payload) {
                    Ok(timing) => {
                        metrics.success = metrics.success.saturating_add(1);
                        metrics.latencies_us.push(timing.total_us);
                        if timing.total_us >= worker_slow_threshold_us {
                            push_top_slow_op(
                                &mut metrics.slow_ops,
                                SlowOpOutlier {
                                    worker_id,
                                    sequence,
                                    handle,
                                    total_ms: us_to_ms(timing.total_us),
                                    write_primary_ms: us_to_ms(timing.write_primary_us),
                                    write_secondary_ms: us_to_ms(timing.write_secondary_us),
                                    verify_read_ms: us_to_ms(timing.verify_read_us),
                                },
                                worker_max_slow_ops,
                            );
                        }
                    }
                    Err(err) => {
                        metrics.failures = metrics.failures.saturating_add(1);
                        if let Some(cause) = retry_after_cause(&err) {
                            metrics.retry_after = metrics.retry_after.saturating_add(1);
                            let entry = metrics.retry_after_by_cause.entry(cause).or_insert(0);
                            *entry = entry.saturating_add(1);
                            let message = retry_after_message_key(&err);
                            let entry = metrics.retry_after_messages.entry(message).or_insert(0);
                            *entry = entry.saturating_add(1);
                        }
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
    timeline_active.store(false, Ordering::Relaxed);
    let _ = telemetry_sampler.join();

    let elapsed = start.elapsed();
    let depth_timeline = timeline_samples
        .lock()
        .map(|items| items.clone())
        .unwrap_or_default();
    let collected = results.lock().expect("metrics lock");
    let mut attempts = 0u64;
    let mut success = 0u64;
    let mut failures = 0u64;
    let mut retry_after = 0u64;
    let mut retry_after_by_cause = BTreeMap::new();
    let mut retry_after_messages = BTreeMap::new();
    let mut latencies = Vec::new();
    let mut slow_ops = Vec::new();
    for metric in collected.iter() {
        attempts = attempts.saturating_add(metric.attempts);
        success = success.saturating_add(metric.success);
        failures = failures.saturating_add(metric.failures);
        retry_after = retry_after.saturating_add(metric.retry_after);
        for (cause, count) in &metric.retry_after_by_cause {
            let entry = retry_after_by_cause.entry(cause.clone()).or_insert(0u64);
            *entry = entry.saturating_add(*count);
        }
        for (message, count) in &metric.retry_after_messages {
            let entry = retry_after_messages.entry(message.clone()).or_insert(0u64);
            *entry = entry.saturating_add(*count);
        }
        latencies.extend_from_slice(&metric.latencies_us);
        for op in &metric.slow_ops {
            push_top_slow_op(&mut slow_ops, op.clone(), max_slow_ops);
        }
    }

    let aggregates: Vec<DbWriteStageAggregate> = metric_handles
        .iter()
        .filter_map(|handle| db_write_stage_aggregate(*handle).ok())
        .collect();
    let health: Vec<DbHealthStatus> = metric_handles
        .iter()
        .filter_map(|handle| db_health_status(*handle).ok())
        .collect();
    let visibility: Vec<DbCommitVisibilityStatus> = metric_handles
        .iter()
        .filter_map(|handle| db_commit_visibility_status(*handle).ok())
        .collect();
    let client_aggregates: Vec<DbClientWritePathAggregate> = metric_handles
        .iter()
        .filter_map(|handle| db_client_write_path_aggregate(*handle).ok())
        .collect();
    let aggregate_count = aggregates.len().max(1) as f64;
    let avg_stage = |selector: fn(&DbWriteStageAggregate) -> f64| -> f64 {
        aggregates.iter().map(selector).sum::<f64>() / aggregate_count
    };

    let mut queue_wait_us = Vec::new();
    let mut lane_dequeue_to_complete_us = Vec::new();
    let mut queue_to_complete_us = Vec::new();
    let mut engine_lock_wait_us = Vec::new();
    let mut validate_route_us = Vec::new();
    let mut replicate_us = Vec::new();
    let mut wal_append_us = Vec::new();
    let mut wal_submit_wait_us = Vec::new();
    let mut wal_hol_wait_us = Vec::new();
    let mut wal_fsync_us = Vec::new();
    let mut apply_us = Vec::new();
    let mut raft_persist_us = Vec::new();
    let mut clock_persist_us = Vec::new();
    let mut total_us = Vec::new();
    for aggregate in &aggregates {
        for sample in &aggregate.recent_samples {
            queue_wait_us.push(sample.queue_wait_ns / 1_000);
            lane_dequeue_to_complete_us.push(sample.lane_dequeue_to_complete_ns / 1_000);
            queue_to_complete_us.push(sample.queue_to_complete_ns / 1_000);
            engine_lock_wait_us.push(sample.engine_lock_wait_ns / 1_000);
            validate_route_us.push(sample.validate_route_ns / 1_000);
            replicate_us.push(sample.replicate_ns / 1_000);
            wal_append_us.push(sample.wal_append_ns / 1_000);
            wal_submit_wait_us.push(sample.wal_submit_wait_ns / 1_000);
            wal_hol_wait_us.push(sample.wal_hol_wait_ns / 1_000);
            wal_fsync_us.push(sample.wal_fdatasync_ns / 1_000);
            apply_us.push(sample.apply_ns / 1_000);
            raft_persist_us.push(sample.raft_persist_ns / 1_000);
            clock_persist_us.push(sample.clock_persist_ns / 1_000);
            total_us.push(sample.total_ns / 1_000);
        }
    }
    let stage_percentiles = StagePercentilesReport {
        queue_wait: percentiles_from_us(&queue_wait_us),
        lane_dequeue_to_complete: percentiles_from_us(&lane_dequeue_to_complete_us),
        queue_to_complete: percentiles_from_us(&queue_to_complete_us),
        engine_lock_wait: percentiles_from_us(&engine_lock_wait_us),
        validate_route: percentiles_from_us(&validate_route_us),
        replicate: percentiles_from_us(&replicate_us),
        wal_append: percentiles_from_us(&wal_append_us),
        wal_submit_wait: percentiles_from_us(&wal_submit_wait_us),
        wal_hol_wait: percentiles_from_us(&wal_hol_wait_us),
        wal_fsync: percentiles_from_us(&wal_fsync_us),
        apply: percentiles_from_us(&apply_us),
        raft_persist: percentiles_from_us(&raft_persist_us),
        clock_persist: percentiles_from_us(&clock_persist_us),
        total: percentiles_from_us(&total_us),
    };
    let mut stage_outliers = Vec::new();
    for aggregate in &aggregates {
        for sample in &aggregate.recent_samples {
            let outlier_max_ns = sample
                .total_ns
                .max(sample.lane_dequeue_to_complete_ns)
                .max(sample.queue_to_complete_ns);
            if outlier_max_ns < stage_outlier_threshold_ms.saturating_mul(1_000_000) {
                continue;
            }
            push_top_stage_outlier(
                &mut stage_outliers,
                StageOutlier {
                    total_ms: ns_to_ms(sample.total_ns),
                    queue_wait_ms: ns_to_ms(sample.queue_wait_ns),
                    lane_dequeue_to_complete_ms: ns_to_ms(sample.lane_dequeue_to_complete_ns),
                    queue_to_complete_ms: ns_to_ms(sample.queue_to_complete_ns),
                    engine_lock_wait_ms: ns_to_ms(sample.engine_lock_wait_ns),
                    validate_route_ms: ns_to_ms(sample.validate_route_ns),
                    replicate_ms: ns_to_ms(sample.replicate_ns),
                    wal_submit_wait_ms: ns_to_ms(sample.wal_submit_wait_ns),
                    wal_hol_wait_ms: ns_to_ms(sample.wal_hol_wait_ns),
                    wal_fsync_ms: ns_to_ms(sample.wal_fdatasync_ns),
                },
                max_stage_outliers,
            );
        }
    }

    let mut client_preflight_us = Vec::new();
    let mut client_enqueue_wait_us = Vec::new();
    let mut client_response_wait_us = Vec::new();
    let mut client_remote_forward_us = Vec::new();
    let mut client_total_us = Vec::new();
    let mut client_path_outliers = Vec::new();
    for aggregate in &client_aggregates {
        for sample in &aggregate.recent_samples {
            client_preflight_us.push(sample.preflight_ns / 1_000);
            client_enqueue_wait_us.push(sample.enqueue_wait_ns / 1_000);
            client_response_wait_us.push(sample.response_wait_ns / 1_000);
            client_remote_forward_us.push(sample.remote_forward_ns / 1_000);
            client_total_us.push(sample.total_ns / 1_000);
            if sample.total_ns < client_path_outlier_threshold_ms.saturating_mul(1_000_000) {
                continue;
            }
            push_top_client_path_outlier(
                &mut client_path_outliers,
                ClientPathOutlier {
                    total_ms: ns_to_ms(sample.total_ns),
                    preflight_ms: ns_to_ms(sample.preflight_ns),
                    enqueue_wait_ms: ns_to_ms(sample.enqueue_wait_ns),
                    response_wait_ms: ns_to_ms(sample.response_wait_ns),
                    remote_forward_ms: ns_to_ms(sample.remote_forward_ns),
                    forwarded: sample.forwarded,
                },
                max_client_path_outliers,
            );
        }
    }
    let client_write_path = ClientWritePathTelemetryReport {
        sample_count: client_aggregates.iter().map(|item| item.sample_count).sum(),
        forwarded_count: client_aggregates
            .iter()
            .map(|item| item.forwarded_count)
            .sum(),
        avg_total_us: weighted_client_path_metric(&client_aggregates, |item| item.avg_total_us),
        preflight_pct: weighted_client_path_metric(&client_aggregates, |item| item.preflight_pct),
        enqueue_wait_pct: weighted_client_path_metric(&client_aggregates, |item| {
            item.enqueue_wait_pct
        }),
        response_wait_pct: weighted_client_path_metric(&client_aggregates, |item| {
            item.response_wait_pct
        }),
        remote_forward_pct: weighted_client_path_metric(&client_aggregates, |item| {
            item.remote_forward_pct
        }),
        other_pct: weighted_client_path_metric(&client_aggregates, |item| item.other_pct),
        preflight_percentiles: percentiles_from_us(&client_preflight_us),
        enqueue_wait_percentiles: percentiles_from_us(&client_enqueue_wait_us),
        response_wait_percentiles: percentiles_from_us(&client_response_wait_us),
        remote_forward_percentiles: percentiles_from_us(&client_remote_forward_us),
        total_percentiles: percentiles_from_us(&client_total_us),
    };

    let queue_depth_values = health
        .iter()
        .map(|item| item.replication_queue_depth)
        .collect::<Vec<_>>();
    let queue_depth_peak_values = health
        .iter()
        .map(|item| item.replication_queue_depth_peak)
        .collect::<Vec<_>>();
    let quorum_replication_latency_values = health
        .iter()
        .map(|item| item.quorum_replication_latency_ns)
        .collect::<Vec<_>>();
    let quorum_fsync_latency_values = health
        .iter()
        .map(|item| item.quorum_fsync_latency_ns)
        .collect::<Vec<_>>();
    let apply_backlog_values = visibility
        .iter()
        .map(|item| item.apply_backlog_depth)
        .collect::<Vec<_>>();
    let apply_backlog_peak_values = health
        .iter()
        .map(|item| item.apply_backlog_peak)
        .collect::<Vec<_>>();
    let replica_ack_count_values = health
        .iter()
        .map(|item| item.replica_acks.len() as u64)
        .collect::<Vec<_>>();
    let replica_durable_ack_count_values = health
        .iter()
        .map(|item| {
            item.replica_acks
                .iter()
                .filter(|ack| ack.durable_ack)
                .count() as u64
        })
        .collect::<Vec<_>>();
    let replica_replication_latency_values = health
        .iter()
        .flat_map(|item| {
            item.replica_acks
                .iter()
                .map(|ack| ack.replication_latency_ns)
        })
        .collect::<Vec<_>>();
    let replica_fsync_latency_values = health
        .iter()
        .flat_map(|item| item.replica_acks.iter().map(|ack| ack.fsync_latency_ns))
        .collect::<Vec<_>>();
    let target_count = health
        .iter()
        .map(|item| item.replication_target_count)
        .max()
        .unwrap_or(0);
    let contacted_count = health
        .iter()
        .map(|item| item.replication_contacted_count)
        .max()
        .unwrap_or(0);
    let skipped_count = health
        .iter()
        .map(|item| item.replication_skipped_count)
        .max()
        .unwrap_or(0);
    let replication = ReplicationTelemetryReport {
        telemetry_sample_period_ms,
        queue_depth: average_u64(&queue_depth_values),
        queue_depth_peak: queue_depth_peak_values.into_iter().max().unwrap_or(0),
        batch_samples: health
            .iter()
            .map(|item| item.replication_batch_samples)
            .sum(),
        batch_ops_le_1: health
            .iter()
            .map(|item| item.replication_batch_ops_le_1)
            .sum(),
        batch_ops_le_4: health
            .iter()
            .map(|item| item.replication_batch_ops_le_4)
            .sum(),
        batch_ops_le_16: health
            .iter()
            .map(|item| item.replication_batch_ops_le_16)
            .sum(),
        batch_ops_le_64: health
            .iter()
            .map(|item| item.replication_batch_ops_le_64)
            .sum(),
        batch_ops_gt_64: health
            .iter()
            .map(|item| item.replication_batch_ops_gt_64)
            .sum(),
        batch_bytes_le_1k: health
            .iter()
            .map(|item| item.replication_batch_bytes_le_1k)
            .sum(),
        batch_bytes_le_4k: health
            .iter()
            .map(|item| item.replication_batch_bytes_le_4k)
            .sum(),
        batch_bytes_le_16k: health
            .iter()
            .map(|item| item.replication_batch_bytes_le_16k)
            .sum(),
        batch_bytes_le_64k: health
            .iter()
            .map(|item| item.replication_batch_bytes_le_64k)
            .sum(),
        batch_bytes_gt_64k: health
            .iter()
            .map(|item| item.replication_batch_bytes_gt_64k)
            .sum(),
        quorum_ack_count: average_u64(
            &health
                .iter()
                .map(|item| item.quorum_ack_count)
                .collect::<Vec<_>>(),
        ),
        quorum_size: average_u64(
            &health
                .iter()
                .map(|item| item.quorum_size)
                .collect::<Vec<_>>(),
        ),
        quorum_replication_ms: percentile_ms_ref(
            &quorum_replication_latency_values
                .into_iter()
                .map(|value| value / 1_000)
                .collect::<Vec<_>>(),
            0.99,
        ),
        quorum_fsync_ms: percentile_ms_ref(
            &quorum_fsync_latency_values
                .into_iter()
                .map(|value| value / 1_000)
                .collect::<Vec<_>>(),
            0.99,
        ),
        target_count,
        contacted_count,
        wave_count: health
            .iter()
            .map(|item| item.replication_wave_count)
            .max()
            .unwrap_or(0),
        wave_avg_targets: average_u64(
            &health
                .iter()
                .map(|item| item.replication_wave_avg_targets)
                .collect::<Vec<_>>(),
        ),
        wave_max_targets: health
            .iter()
            .map(|item| item.replication_wave_max_targets)
            .max()
            .unwrap_or(0),
        successful_count: health
            .iter()
            .map(|item| item.replication_successful_count)
            .max()
            .unwrap_or(0),
        failed_count: health
            .iter()
            .map(|item| item.replication_failed_count)
            .max()
            .unwrap_or(0),
        cancelled_count: health
            .iter()
            .map(|item| item.replication_cancelled_count)
            .max()
            .unwrap_or(0),
        contact_efficiency_bps: health
            .iter()
            .map(|item| item.replication_contact_efficiency_bps)
            .max()
            .unwrap_or(0),
        target_efficiency_bps: health
            .iter()
            .map(|item| item.replication_target_efficiency_bps)
            .max()
            .unwrap_or(0),
        skipped_count,
        aborted_in_flight_count: health
            .iter()
            .map(|item| item.replication_aborted_in_flight_count)
            .max()
            .unwrap_or(0),
        contacted_ratio_pct: if target_count == 0 {
            0.0
        } else {
            (contacted_count as f64) * 100.0 / (target_count as f64)
        },
        skipped_ratio_pct: if target_count == 0 {
            0.0
        } else {
            (skipped_count as f64) * 100.0 / (target_count as f64)
        },
        simulation_commits: health
            .iter()
            .map(|item| item.replication_simulation_commits)
            .max()
            .unwrap_or(0),
        rpc_max_in_flight: health
            .iter()
            .map(|item| item.replication_rpc_max_in_flight)
            .max()
            .unwrap_or(0),
        rpc_in_flight: health
            .iter()
            .map(|item| item.replication_rpc_in_flight)
            .max()
            .unwrap_or(0),
        rpc_available_permits: health
            .iter()
            .map(|item| item.replication_rpc_available_permits)
            .min()
            .unwrap_or(0),
        rpc_backpressure_timeouts: health
            .iter()
            .map(|item| item.replication_rpc_backpressure_timeouts)
            .max()
            .unwrap_or(0),
        rpc_backpressure_closed: health
            .iter()
            .map(|item| item.replication_rpc_backpressure_closed)
            .max()
            .unwrap_or(0),
        real_quorum_evidence: target_count > 0
            && contacted_count > 0
            && health
                .iter()
                .all(|item| item.replication_simulation_commits == 0),
        quorum_transport_mode: health
            .first()
            .map(|item| quorum_transport_mode_str(item.quorum_transport_mode).to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        replicated_log_backend: health
            .first()
            .map(|item| replicated_log_backend_str(item.replicated_log_backend).to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        replicated_log_shadow_payload_bytes: health
            .iter()
            .map(|item| item.replicated_log_shadow_payload_bytes)
            .max()
            .unwrap_or(0),
        replicated_log_shadow_wal_bytes: health
            .iter()
            .map(|item| item.replicated_log_shadow_wal_bytes)
            .max()
            .unwrap_or(0),
        replicated_log_shadow_overhead_bytes: health
            .iter()
            .map(|item| item.replicated_log_shadow_overhead_bytes)
            .max()
            .unwrap_or(0),
        replica_ack_count: average_u64(&replica_ack_count_values),
        replica_durable_ack_count: average_u64(&replica_durable_ack_count_values),
        replica_max_replication_ms: percentile_ms_ref(
            &replica_replication_latency_values
                .into_iter()
                .map(|value| value / 1_000)
                .collect::<Vec<_>>(),
            0.99,
        ),
        replica_max_fsync_ms: percentile_ms_ref(
            &replica_fsync_latency_values
                .into_iter()
                .map(|value| value / 1_000)
                .collect::<Vec<_>>(),
            0.99,
        ),
        apply_backlog_depth: average_u64(&apply_backlog_values),
        apply_backlog_peak: apply_backlog_peak_values.into_iter().max().unwrap_or(0),
        depth_timeline,
        quorum_failure_token: health
            .iter()
            .find_map(|item| item.quorum_failure_token.clone()),
        quorum_failure_reason: health
            .iter()
            .find_map(|item| item.quorum_failure_reason.clone()),
        failure_counters: {
            let mut counters = BTreeMap::new();
            for item in &health {
                for counter in &item.replication_failure_counters {
                    let entry = counters.entry(counter.token.clone()).or_insert(0u64);
                    *entry = entry.saturating_add(counter.count);
                }
            }
            counters
        },
    };
    let writer_lanes = WriterLaneTelemetryReport {
        lane_count: health
            .iter()
            .map(|item| item.writer_lanes.len() as u64)
            .max()
            .unwrap_or(0),
        active_lane_count: health
            .iter()
            .map(|item| {
                item.writer_lanes
                    .iter()
                    .filter(|lane| lane.assigned_shards > 0)
                    .count() as u64
            })
            .max()
            .unwrap_or(0),
        total_assigned_shards: health
            .iter()
            .map(|item| {
                item.writer_lanes
                    .iter()
                    .map(|lane| lane.assigned_shards)
                    .sum::<u64>()
            })
            .max()
            .unwrap_or(0),
        max_assigned_shards: health
            .iter()
            .flat_map(|item| item.writer_lanes.iter().map(|lane| lane.assigned_shards))
            .max()
            .unwrap_or(0),
        max_assigned_shard_share_pct: health
            .iter()
            .map(|item| {
                let total_assigned = item
                    .writer_lanes
                    .iter()
                    .map(|lane| lane.assigned_shards)
                    .sum::<u64>();
                if total_assigned == 0 {
                    return 0.0;
                }
                let max_assigned = item
                    .writer_lanes
                    .iter()
                    .map(|lane| lane.assigned_shards)
                    .max()
                    .unwrap_or(0);
                (max_assigned as f64) * 100.0 / (total_assigned as f64)
            })
            .fold(0.0, f64::max),
        max_queue_depth: health
            .iter()
            .flat_map(|item| item.writer_lanes.iter().map(|lane| lane.queue_depth))
            .max()
            .unwrap_or(0),
        max_lane_retry_after_bps: health
            .iter()
            .map(|item| item.writer_lane_max_retry_after_bps)
            .max()
            .unwrap_or(0),
        max_lane_saturation_bps: health
            .iter()
            .map(|item| item.writer_lane_max_saturation_bps)
            .max()
            .unwrap_or(0),
        max_enqueue_attempt_share_bps: health
            .iter()
            .map(|item| item.writer_lane_max_enqueue_share_bps)
            .max()
            .unwrap_or(0),
        assignment_lookups: health
            .iter()
            .map(|item| item.writer_lane_assignment_lookups)
            .max()
            .unwrap_or(0),
        assignment_hits: health
            .iter()
            .map(|item| item.writer_lane_assignment_hits)
            .max()
            .unwrap_or(0),
        assignment_misses: health
            .iter()
            .map(|item| item.writer_lane_assignment_misses)
            .max()
            .unwrap_or(0),
        assignment_hit_rate_bps: health
            .iter()
            .map(|item| item.writer_lane_assignment_hit_rate_bps)
            .max()
            .unwrap_or(0),
        max_lane_retry_after_pct: health
            .iter()
            .map(|item| item.writer_lane_max_retry_after_bps as f64 / 100.0)
            .fold(0.0, f64::max),
        max_lane_saturation_pct: health
            .iter()
            .map(|item| item.writer_lane_max_saturation_bps as f64 / 100.0)
            .fold(0.0, f64::max),
        max_enqueue_attempt_share_pct: health
            .iter()
            .map(|item| item.writer_lane_max_enqueue_share_bps as f64 / 100.0)
            .fold(0.0, f64::max),
        assignment_hit_rate_pct: health
            .iter()
            .map(|item| item.writer_lane_assignment_hit_rate_bps as f64 / 100.0)
            .fold(0.0, f64::max),
    };
    let apply_lanes = ApplyLaneTelemetryReport {
        lane_count: health
            .iter()
            .map(|item| item.apply_lanes.len() as u64)
            .max()
            .unwrap_or(0),
        active_lane_count: health
            .iter()
            .map(|item| {
                item.apply_lanes
                    .iter()
                    .filter(|lane| lane.queue_depth > 0)
                    .count() as u64
            })
            .max()
            .unwrap_or(0),
        max_queue_depth: health
            .iter()
            .map(|item| item.apply_lane_max_queue_depth)
            .max()
            .unwrap_or(0),
    };
    let lsm = LsmTelemetryReport {
        compaction_debt_bytes_estimate: health
            .iter()
            .map(|item| item.lsm_compaction_debt_bytes_estimate)
            .max()
            .unwrap_or(0),
        shadow_bytes_estimate: health
            .iter()
            .map(|item| item.lsm_shadow_bytes_estimate)
            .max()
            .unwrap_or(0),
        live_bytes_estimate: health
            .iter()
            .map(|item| item.lsm_live_bytes_estimate)
            .max()
            .unwrap_or(0),
        total_bytes_estimate: health
            .iter()
            .map(|item| item.lsm_total_bytes_estimate)
            .max()
            .unwrap_or(0),
        version_count: health
            .iter()
            .map(|item| item.lsm_version_count)
            .max()
            .unwrap_or(0),
        tombstone_count: health
            .iter()
            .map(|item| item.lsm_tombstone_count)
            .max()
            .unwrap_or(0),
    };

    WorkloadReport {
        schema_version: WORKLOAD_SCHEMA_VERSION,
        name: kind.as_str().to_string(),
        concurrency,
        duration_seconds: duration.as_secs(),
        payload_bytes,
        attempts,
        success,
        failures,
        retry_after_pct: if attempts == 0 {
            0.0
        } else {
            (retry_after as f64) * 100.0 / (attempts as f64)
        },
        retry_after_by_cause,
        retry_after_messages: top_count_map(retry_after_messages, 10),
        tps: if elapsed.is_zero() {
            0.0
        } else {
            success as f64 / elapsed.as_secs_f64()
        },
        p50_ms: percentile_ms(latencies.clone(), 0.50),
        p95_ms: percentile_ms(latencies.clone(), 0.95),
        p99_ms: percentile_ms(latencies.clone(), 0.99),
        p999_ms: percentile_ms(latencies.clone(), 0.999),
        stage_avg_queue_wait_ms: avg_stage(|stage| stage.avg_queue_wait_us) / 1_000.0,
        stage_avg_lane_dequeue_to_complete_ms: avg_stage(|stage| {
            stage.avg_lane_dequeue_to_complete_us
        }) / 1_000.0,
        stage_avg_queue_to_complete_ms: avg_stage(|stage| stage.avg_queue_to_complete_us) / 1_000.0,
        queue_saturation_pct: avg_stage(|stage| stage.queue_saturation_pct),
        stage_engine_lock_wait_pct: avg_stage(|stage| stage.engine_lock_wait_pct),
        stage_validate_route_pct: avg_stage(|stage| stage.validate_route_pct),
        stage_replicate_pct: avg_stage(|stage| stage.replicate_pct),
        stage_wal_append_pct: avg_stage(|stage| stage.wal_append_pct),
        stage_wal_submit_wait_pct: avg_stage(|stage| stage.wal_submit_wait_pct),
        stage_wal_hol_wait_pct: avg_stage(|stage| stage.wal_hol_wait_pct),
        stage_wal_queue_wait_pct: avg_stage(|stage| stage.wal_queue_wait_pct),
        stage_wal_encode_pct: avg_stage(|stage| stage.wal_encode_pct),
        stage_wal_fdatasync_pct: avg_stage(|stage| stage.wal_fdatasync_pct),
        stage_wal_mutex_wait_pct: avg_stage(|stage| stage.wal_mutex_wait_pct),
        stage_apply_pct: avg_stage(|stage| stage.apply_pct),
        stage_raft_persist_pct: avg_stage(|stage| stage.raft_persist_pct),
        stage_clock_persist_pct: avg_stage(|stage| stage.clock_persist_pct),
        stage_other_pct: avg_stage(|stage| stage.other_pct),
        stage_percentiles,
        client_write_path,
        replication,
        writer_lanes,
        apply_lanes,
        lsm,
        latency_histogram: latency_histogram(&latencies),
        slow_op_threshold_ms,
        slow_ops,
        stage_outlier_threshold_ms,
        stage_outliers,
        client_path_outlier_threshold_ms,
        client_path_outliers,
    }
}

fn assert_workload_schema(report: &WorkloadReport) {
    let value = serde_json::to_value(report).expect("serialize workload report");
    assert_eq!(
        value
            .get("schema_version")
            .and_then(Value::as_u64)
            .expect("schema_version"),
        WORKLOAD_SCHEMA_VERSION as u64
    );
    for key in [
        "name",
        "concurrency",
        "duration_seconds",
        "retry_after_by_cause",
        "tps",
        "p99_ms",
        "p999_ms",
        "stage_percentiles",
        "client_write_path",
        "replication",
        "writer_lanes",
        "apply_lanes",
        "lsm",
        "latency_histogram",
        "slow_ops",
        "stage_outliers",
        "client_path_outliers",
    ] {
        assert!(
            value.get(key).is_some(),
            "workload report missing required key {key}"
        );
    }
    assert!(
        value
            .get("stage_percentiles")
            .and_then(|item| item.get("total"))
            .and_then(|item| item.get("p999_ms"))
            .and_then(Value::as_f64)
            .is_some(),
        "stage percentiles schema drift: total.p999_ms missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("queue_depth"))
            .and_then(Value::as_u64)
            .is_some(),
        "replication schema drift: queue_depth missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("replica_ack_count"))
            .and_then(Value::as_u64)
            .is_some(),
        "replication schema drift: replica_ack_count missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("quorum_failure_token"))
            .is_some(),
        "replication schema drift: quorum_failure_token missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("target_count"))
            .and_then(Value::as_u64)
            .is_some(),
        "replication schema drift: target_count missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("contacted_count"))
            .and_then(Value::as_u64)
            .is_some(),
        "replication schema drift: contacted_count missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("wave_count"))
            .and_then(Value::as_u64)
            .is_some(),
        "replication schema drift: wave_count missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("wave_avg_targets"))
            .and_then(Value::as_u64)
            .is_some(),
        "replication schema drift: wave_avg_targets missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("wave_max_targets"))
            .and_then(Value::as_u64)
            .is_some(),
        "replication schema drift: wave_max_targets missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("successful_count"))
            .and_then(Value::as_u64)
            .is_some(),
        "replication schema drift: successful_count missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("failed_count"))
            .and_then(Value::as_u64)
            .is_some(),
        "replication schema drift: failed_count missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("cancelled_count"))
            .and_then(Value::as_u64)
            .is_some(),
        "replication schema drift: cancelled_count missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("contact_efficiency_bps"))
            .and_then(Value::as_u64)
            .is_some(),
        "replication schema drift: contact_efficiency_bps missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("target_efficiency_bps"))
            .and_then(Value::as_u64)
            .is_some(),
        "replication schema drift: target_efficiency_bps missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("skipped_count"))
            .and_then(Value::as_u64)
            .is_some(),
        "replication schema drift: skipped_count missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("aborted_in_flight_count"))
            .and_then(Value::as_u64)
            .is_some(),
        "replication schema drift: aborted_in_flight_count missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("contacted_ratio_pct"))
            .and_then(Value::as_f64)
            .is_some(),
        "replication schema drift: contacted_ratio_pct missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("skipped_ratio_pct"))
            .and_then(Value::as_f64)
            .is_some(),
        "replication schema drift: skipped_ratio_pct missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("simulation_commits"))
            .and_then(Value::as_u64)
            .is_some(),
        "replication schema drift: simulation_commits missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("rpc_max_in_flight"))
            .and_then(Value::as_u64)
            .is_some(),
        "replication schema drift: rpc_max_in_flight missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("rpc_in_flight"))
            .and_then(Value::as_u64)
            .is_some(),
        "replication schema drift: rpc_in_flight missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("rpc_available_permits"))
            .and_then(Value::as_u64)
            .is_some(),
        "replication schema drift: rpc_available_permits missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("rpc_backpressure_timeouts"))
            .and_then(Value::as_u64)
            .is_some(),
        "replication schema drift: rpc_backpressure_timeouts missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("rpc_backpressure_closed"))
            .and_then(Value::as_u64)
            .is_some(),
        "replication schema drift: rpc_backpressure_closed missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("real_quorum_evidence"))
            .and_then(Value::as_bool)
            .is_some(),
        "replication schema drift: real_quorum_evidence missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("batch_bytes_le_1k"))
            .and_then(Value::as_u64)
            .is_some(),
        "replication schema drift: batch_bytes_le_1k missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("batch_bytes_gt_64k"))
            .and_then(Value::as_u64)
            .is_some(),
        "replication schema drift: batch_bytes_gt_64k missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("quorum_transport_mode"))
            .and_then(Value::as_str)
            .is_some(),
        "replication schema drift: quorum_transport_mode missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("replicated_log_backend"))
            .and_then(Value::as_str)
            .is_some(),
        "replication schema drift: replicated_log_backend missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("replicated_log_shadow_overhead_bytes"))
            .and_then(Value::as_u64)
            .is_some(),
        "replication schema drift: replicated_log_shadow_overhead_bytes missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("failure_counters"))
            .and_then(Value::as_object)
            .is_some(),
        "replication schema drift: failure_counters missing"
    );
    assert!(
        value
            .get("replication")
            .and_then(|item| item.get("telemetry_sample_period_ms"))
            .and_then(Value::as_u64)
            .is_some(),
        "replication schema drift: telemetry_sample_period_ms missing"
    );
    assert!(
        value
            .get("writer_lanes")
            .and_then(|item| item.get("lane_count"))
            .and_then(Value::as_u64)
            .is_some(),
        "writer_lanes schema drift: lane_count missing"
    );
    assert!(
        value
            .get("writer_lanes")
            .and_then(|item| item.get("active_lane_count"))
            .and_then(Value::as_u64)
            .is_some(),
        "writer_lanes schema drift: active_lane_count missing"
    );
    assert!(
        value
            .get("writer_lanes")
            .and_then(|item| item.get("total_assigned_shards"))
            .and_then(Value::as_u64)
            .is_some(),
        "writer_lanes schema drift: total_assigned_shards missing"
    );
    assert!(
        value
            .get("writer_lanes")
            .and_then(|item| item.get("max_assigned_shards"))
            .and_then(Value::as_u64)
            .is_some(),
        "writer_lanes schema drift: max_assigned_shards missing"
    );
    assert!(
        value
            .get("writer_lanes")
            .and_then(|item| item.get("max_enqueue_attempt_share_bps"))
            .and_then(Value::as_u64)
            .is_some(),
        "writer_lanes schema drift: max_enqueue_attempt_share_bps missing"
    );
    assert!(
        value
            .get("writer_lanes")
            .and_then(|item| item.get("assignment_lookups"))
            .and_then(Value::as_u64)
            .is_some(),
        "writer_lanes schema drift: assignment_lookups missing"
    );
    assert!(
        value
            .get("writer_lanes")
            .and_then(|item| item.get("assignment_hits"))
            .and_then(Value::as_u64)
            .is_some(),
        "writer_lanes schema drift: assignment_hits missing"
    );
    assert!(
        value
            .get("writer_lanes")
            .and_then(|item| item.get("assignment_misses"))
            .and_then(Value::as_u64)
            .is_some(),
        "writer_lanes schema drift: assignment_misses missing"
    );
    assert!(
        value
            .get("writer_lanes")
            .and_then(|item| item.get("assignment_hit_rate_bps"))
            .and_then(Value::as_u64)
            .is_some(),
        "writer_lanes schema drift: assignment_hit_rate_bps missing"
    );
    assert!(
        value
            .get("apply_lanes")
            .and_then(|item| item.get("lane_count"))
            .and_then(Value::as_u64)
            .is_some(),
        "apply_lanes schema drift: lane_count missing"
    );
    assert!(
        value
            .get("apply_lanes")
            .and_then(|item| item.get("active_lane_count"))
            .and_then(Value::as_u64)
            .is_some(),
        "apply_lanes schema drift: active_lane_count missing"
    );
    assert!(
        value
            .get("apply_lanes")
            .and_then(|item| item.get("max_queue_depth"))
            .and_then(Value::as_u64)
            .is_some(),
        "apply_lanes schema drift: max_queue_depth missing"
    );
    assert!(
        value
            .get("lsm")
            .and_then(|item| item.get("compaction_debt_bytes_estimate"))
            .and_then(Value::as_u64)
            .is_some(),
        "lsm schema drift: compaction_debt_bytes_estimate missing"
    );
    assert!(
        value
            .get("lsm")
            .and_then(|item| item.get("shadow_bytes_estimate"))
            .and_then(Value::as_u64)
            .is_some(),
        "lsm schema drift: shadow_bytes_estimate missing"
    );
    let depth_timeline = value
        .get("replication")
        .and_then(|item| item.get("depth_timeline"))
        .and_then(Value::as_array)
        .expect("replication schema drift: depth_timeline missing");
    if let Some(first_sample) = depth_timeline.first() {
        assert!(
            first_sample
                .get("elapsed_ms")
                .and_then(Value::as_u64)
                .is_some(),
            "replication timeline schema drift: elapsed_ms missing"
        );
        assert!(
            first_sample
                .get("apply_backlog_depth")
                .and_then(Value::as_u64)
                .is_some(),
            "replication timeline schema drift: apply_backlog_depth missing"
        );
    }
    assert!(
        value
            .get("client_write_path")
            .and_then(|item| item.get("response_wait_pct"))
            .and_then(Value::as_f64)
            .is_some(),
        "client write path schema drift: response_wait_pct missing"
    );
}

fn assert_summary_schema(summary: &Value) {
    assert_eq!(
        summary
            .get("schema_version")
            .and_then(Value::as_u64)
            .expect("summary.schema_version"),
        SUMMARY_SCHEMA_VERSION as u64
    );
    assert!(
        summary
            .get("run_metadata")
            .and_then(|item| item.get("os"))
            .and_then(Value::as_str)
            .is_some(),
        "summary schema drift: run_metadata.os missing"
    );
    assert!(
        summary
            .get("run_metadata")
            .and_then(|item| item.get("arch"))
            .and_then(Value::as_str)
            .is_some(),
        "summary schema drift: run_metadata.arch missing"
    );
    assert!(
        summary
            .get("config")
            .and_then(|item| item.get("writer_lane_count"))
            .and_then(Value::as_u64)
            .is_some(),
        "summary schema drift: config.writer_lane_count missing"
    );
    assert!(
        summary
            .get("config")
            .and_then(|item| item.get("apply_lane_count"))
            .and_then(Value::as_u64)
            .is_some(),
        "summary schema drift: config.apply_lane_count missing"
    );
    assert!(
        summary
            .get("config")
            .and_then(|item| item.get("private_rpc_channels_per_target"))
            .and_then(Value::as_u64)
            .is_some(),
        "summary schema drift: config.private_rpc_channels_per_target missing"
    );
    assert!(
        summary
            .get("config")
            .and_then(|item| item.get("replication_max_in_flight"))
            .and_then(Value::as_u64)
            .is_some(),
        "summary schema drift: config.replication_max_in_flight missing"
    );
    assert!(
        summary
            .get("config")
            .and_then(|item| item.get("replication_batch_max_ops"))
            .and_then(Value::as_u64)
            .is_some(),
        "summary schema drift: config.replication_batch_max_ops missing"
    );
    assert!(
        summary
            .get("config")
            .and_then(|item| item.get("replication_batch_max_bytes"))
            .and_then(Value::as_u64)
            .is_some(),
        "summary schema drift: config.replication_batch_max_bytes missing"
    );
    assert!(
        summary
            .get("config")
            .and_then(|item| item.get("replication_max_targets"))
            .and_then(Value::as_u64)
            .is_some(),
        "summary schema drift: config.replication_max_targets missing"
    );
    assert!(
        summary
            .get("config")
            .and_then(|item| item.get("replication_hedge_extra"))
            .and_then(Value::as_u64)
            .is_some(),
        "summary schema drift: config.replication_hedge_extra missing"
    );
    assert!(
        summary
            .get("config")
            .and_then(|item| item.get("logical_shards"))
            .and_then(Value::as_u64)
            .is_some(),
        "summary schema drift: config.logical_shards missing"
    );
    assert!(
        summary
            .get("config")
            .and_then(|item| item.get("active_groups"))
            .and_then(Value::as_u64)
            .is_some(),
        "summary schema drift: config.active_groups missing"
    );
    assert!(
        summary
            .get("config")
            .and_then(|item| item.get("replication_factor"))
            .and_then(Value::as_u64)
            .is_some(),
        "summary schema drift: config.replication_factor missing"
    );
    assert!(
        summary
            .get("config")
            .and_then(|item| item.get("write_quorum"))
            .and_then(Value::as_u64)
            .is_some(),
        "summary schema drift: config.write_quorum missing"
    );
    assert!(
        summary
            .get("config")
            .and_then(|item| item.get("quorum_transport_mode"))
            .and_then(Value::as_str)
            .is_some(),
        "summary schema drift: config.quorum_transport_mode missing"
    );
    assert!(
        summary
            .get("config")
            .and_then(|item| item.get("replicated_log_backend"))
            .and_then(Value::as_str)
            .is_some(),
        "summary schema drift: config.replicated_log_backend missing"
    );
    assert!(
        summary
            .get("config")
            .and_then(|item| item.get("real_quorum_mode"))
            .and_then(Value::as_bool)
            .is_some(),
        "summary schema drift: config.real_quorum_mode missing"
    );
    assert!(
        summary
            .get("config")
            .and_then(|item| item.get("require_lane_spread"))
            .and_then(Value::as_bool)
            .is_some(),
        "summary schema drift: config.require_lane_spread missing"
    );
}

#[test]
#[ignore = "manual local perf harness; run explicitly for throughput measurements"]
fn db_write_local_perf_harness_emits_json_artifacts() {
    let duration_seconds = env_u64("WRELA_LOCAL_PERF_DURATION_SECONDS", 20);
    let concurrency = env_usize("WRELA_LOCAL_PERF_CONCURRENCY", 32);
    let payload_bytes = env_usize("WRELA_LOCAL_PERF_PAYLOAD_BYTES", 128);
    let writer_lane_count = env_usize("WRELA_LOCAL_PERF_WRITER_LANE_COUNT", 1);
    let apply_lane_count = env_usize(
        "WRELA_LOCAL_PERF_APPLY_LANE_COUNT",
        writer_lane_count.max(1),
    );
    let private_rpc_channels_per_target = env_usize("WRELA_LOCAL_PERF_RPC_CHANNELS_PER_TARGET", 1);
    let logical_shards = env_usize("WRELA_LOCAL_PERF_LOGICAL_SHARDS", 1);
    let active_groups = env_usize("WRELA_LOCAL_PERF_ACTIVE_GROUPS", 1);
    let replication_batch_window_us = env_u64("WRELA_LOCAL_PERF_BATCH_WINDOW_US", 500);
    let replication_batch_max_ops = env_usize("WRELA_LOCAL_PERF_BATCH_MAX_OPS", 64);
    let replication_batch_max_bytes = env_usize("WRELA_LOCAL_PERF_BATCH_MAX_BYTES", 256 * 1024);
    let replication_max_in_flight = env_usize("WRELA_LOCAL_PERF_MAX_IN_FLIGHT", 32);
    let replication_max_targets = env_usize("WRELA_LOCAL_PERF_MAX_TARGETS", 256);
    let replication_hedge_extra = env_usize("WRELA_LOCAL_PERF_HEDGE_EXTRA", 1);
    let replication_factor = env_u64("WRELA_LOCAL_PERF_REPLICATION_FACTOR", 3);
    let write_quorum = env_u64("WRELA_LOCAL_PERF_WRITE_QUORUM", 2);
    let commit_visibility_mode = std::env::var("WRELA_LOCAL_PERF_COMMIT_VISIBILITY_MODE")
        .ok()
        .unwrap_or_else(|| "async_apply".to_string());
    let quorum_transport_mode = "require_private_rpc".to_string();
    let replicated_log_backend = std::env::var("WRELA_LOCAL_PERF_LOG_BACKEND")
        .ok()
        .unwrap_or_else(|| "dual_wal".to_string());
    let real_quorum_mode = true;
    let require_lane_spread = env_bool("WRELA_LOCAL_PERF_REQUIRE_LANE_SPREAD", false);
    let effective_quorum_transport_mode = quorum_transport_mode.clone();

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
        .join(&run_id);
    fs::create_dir_all(&artifact_dir).expect("create artifact directory");

    let duration = Duration::from_secs(duration_seconds);
    let mut workloads = Vec::new();
    for kind in [
        WorkloadKind::RawLeaderLocal,
        WorkloadKind::RawRoundRobinNodes,
        WorkloadKind::ValidatedWritePath,
    ] {
        let fixture = open_real_quorum_fixture(kind);
        workloads.push(run_workload(
            kind,
            &fixture.workload_handles,
            concurrency,
            duration,
            payload_bytes,
        ));
        fixture.teardown();
    }

    for workload in &workloads {
        assert!(
            workload.replication.real_quorum_evidence,
            "anti-cheat failed for {}: expected real quorum evidence (non-simulated replication path)",
            workload.name
        );
        assert_eq!(
            workload.replication.simulation_commits, 0,
            "anti-cheat failed for {}: simulation commits must be zero in strict real quorum mode",
            workload.name
        );
        if workload.replication.replicated_log_backend == "shadow_canonical" {
            assert!(
                workload.replication.replicated_log_shadow_payload_bytes > 0,
                "anti-cheat failed for {}: shadow backend mode must report non-zero payload bytes",
                workload.name
            );
            assert!(
                workload.replication.replicated_log_shadow_wal_bytes
                    >= workload.replication.replicated_log_shadow_payload_bytes,
                "anti-cheat failed for {}: shadow wal bytes must be >= payload bytes",
                workload.name
            );
        }
        if require_lane_spread && writer_lane_count > 1 {
            assert!(
                workload.writer_lanes.active_lane_count >= 2,
                "anti-cheat failed for {}: lane spread required but active_lane_count < 2",
                workload.name
            );
            assert!(
                workload.writer_lanes.total_assigned_shards >= 2,
                "anti-cheat failed for {}: lane spread required but total_assigned_shards < 2",
                workload.name
            );
            assert!(
                workload.writer_lanes.max_assigned_shard_share_pct < 100.0,
                "anti-cheat failed for {}: lane spread required but max_assigned_shard_share_pct >= 100",
                workload.name
            );
        }
        assert_workload_schema(workload);
        let file_path = artifact_dir.join(format!("{}.json", workload.name));
        let body = serde_json::to_string_pretty(workload).expect("serialize workload report");
        fs::write(file_path, body).expect("write workload report");
    }

    let summary_path = artifact_dir.join("summary.json");
    let summary = json!({
        "schema_version": SUMMARY_SCHEMA_VERSION,
        "run_id": run_id,
        "generated_at_epoch_ms": SystemTime::now().duration_since(UNIX_EPOCH).map(|dur| dur.as_millis() as u64).unwrap_or(0),
        "artifacts_dir": artifact_dir.to_string_lossy().to_string(),
        "run_metadata": run_metadata_json(),
        "config": {
            "duration_seconds": duration_seconds,
            "concurrency": concurrency,
            "payload_bytes": payload_bytes,
            "writer_lane_count": writer_lane_count,
            "apply_lane_count": apply_lane_count,
            "private_rpc_channels_per_target": private_rpc_channels_per_target,
            "logical_shards": logical_shards,
            "active_groups": active_groups,
            "replication_batch_window_us": replication_batch_window_us,
            "replication_batch_max_ops": replication_batch_max_ops,
            "replication_batch_max_bytes": replication_batch_max_bytes,
            "replication_max_in_flight": replication_max_in_flight,
            "replication_max_targets": replication_max_targets,
            "replication_hedge_extra": replication_hedge_extra,
            "replication_factor": replication_factor,
            "write_quorum": write_quorum,
            "commit_visibility_mode": commit_visibility_mode,
            "quorum_transport_mode": effective_quorum_transport_mode,
            "replicated_log_backend": replicated_log_backend,
            "real_quorum_mode": real_quorum_mode,
            "require_lane_spread": require_lane_spread,
        },
        "workloads": workloads,
    });
    assert_summary_schema(&summary);
    fs::write(
        &summary_path,
        serde_json::to_string_pretty(&summary).expect("serialize summary"),
    )
    .expect("write summary");

    eprintln!(
        "local write perf artifacts written to {}",
        artifact_dir.display()
    );
}
