use crate::db::cdc::CdcEmitter;
use crate::db::hlc::HybridLogicalClock;
use crate::db::keyspace::{
    EncodedUserKey, decode_user_key, encode_user_key, encode_user_key_smallvec,
};
use crate::db::lsm::blob_store::{BlobStore, ValuePlacement};
use crate::db::mvcc::memtable::{Memtable, MemtableStats};
use crate::db::mvcc::occ::validate_expected_version;
use crate::db::raft::append::handle_append_entries;
use crate::db::raft::membership::{MembershipChange, MembershipConfig};
use crate::db::raft::message::{AppendEntries, AppendEntriesResponse, LogEntry};
use crate::db::raft::persistence::{
    PersistedRaftState, RaftPersistMetadata, load_persisted_raft_state, load_raft_metadata_binary,
    persist_raft_metadata_binary, persist_raft_state,
};
use crate::db::raft::pipeline::{RaftAppendFrame, RaftCommand, build_append_frame};
use crate::db::raft::state::NodeState;
use crate::db::read::iterator::{RangeCancellation, RangeIterator};
use crate::db::read::rejection::{StrongReadErrorCode, enforce_strong_read};
use crate::db::read::{PointShortcutPolicy, ReadPath, ReadPathStats};
use crate::db::replication::ack::{LeaderAckInput, evaluate_leader_ack};
use crate::db::replication::catchup::{CatchUpTransferMode, select_transfer_mode};
use crate::db::replication::quorum::{FollowerAppendResponse, response_is_durable_ack};
use crate::db::security::hardening::resolve_private_rpc_security_policy;
use crate::db::security::residency::{ReadSovereigntyMode, ResidencyPolicy};
use crate::db::shard::directory::{ShardDirectory, ShardDirectoryError, ShardRoute};
use crate::db::time::persistence::{load_hlc_state, persist_hlc_state};
use crate::db::time::safe_time::{SafeTimeDiagnostics, SafeTimeLagBudget, SafeTimePropagator};
use crate::db::time::uncertainty::{UncertaintyTracker, UncertaintyWindow};
use crate::db::time::watermarks::SafeReadWatermarks;
use crate::db::topology::persistence::{
    PersistedAutoscaleStatus, PersistedGroupState, PersistedTopologyState,
    load_persisted_topology_state, persist_topology_state,
};
#[cfg(test)]
use crate::db::txn::lock_table::LockTableSnapshot;
use crate::db::txn::lock_table::{LockAcquireOutcome, TxnLockTable};
use crate::db::types::{
    BatchOp, DbError, MAX_BATCH_BYTES, MAX_BATCH_OPS, MAX_KEY_BYTES, MAX_VALUE_BYTES,
};
use crate::db::wal::format::HEADER_BYTES;
use crate::db::wal::format::{
    Record, RecordKind, decode_raft_meta_value, encode_to, record_from_raft_meta,
};
use crate::db::wal::recovery::recover;
use crate::db::wal::segment::{WalBatchCompletion, WalFlushStats, WalSegment};
use crate::kernel::runtime;
use bytes::Bytes;
use crossbeam_channel::{Receiver as WalCompletionReceiver, TryRecvError};
use serde_json::Value as JsonValue;
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::fs::{File, OpenOptions};
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{
    Arc, Condvar, Mutex, MutexGuard, OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

fn runtime_startup_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("WRELA_RUNTIME_STARTUP_TRACE")
            .ok()
            .map(|raw| {
                matches!(
                    raw.trim(),
                    "1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON"
                )
            })
            .unwrap_or(false)
    })
}

fn runtime_startup_trace(message: impl AsRef<str>) {
    if runtime_startup_trace_enabled() {
        eprintln!("[runtime-db] {}", message.as_ref());
    }
}

const SORTED_RUN_CATCHUP_LAG_THRESHOLD_OPS: u64 = 4096;
const SORTED_RUN_CATCHUP_CHUNK_MAX_ENTRIES: usize = 256;
const SORTED_RUN_CATCHUP_CHUNK_MAX_BYTES: usize = 256 * 1024;
const COMPACTION_SCHEDULER_MAX_DEBT_BYTES: u64 = 128 * 1024 * 1024;
const BLOB_VALUE_THRESHOLD_BYTES: usize = 4096;
const DEFAULT_LSM_STATS_REFRESH_OPS_INTERVAL: u64 = 128;
const DEFAULT_BLOB_GC_OPS_INTERVAL: u64 = 256;

/// Reserved namespace for replicated idempotency records. Writes and reads
/// to this namespace bypass residency checks and are replicated with the batch.
pub(crate) const IDEMPOTENCY_NAMESPACE: &[u8] = b"__idempotency";

fn replication_outside_lock_active() -> bool {
    true
}

fn wal_encode_outside_lock_active() -> bool {
    true
}

fn sorted_run_catchup_active() -> bool {
    true
}

fn sorted_run_catchup_lag_threshold_ops_default() -> u64 {
    SORTED_RUN_CATCHUP_LAG_THRESHOLD_OPS
}

fn sorted_run_catchup_chunk_max_entries_default() -> usize {
    SORTED_RUN_CATCHUP_CHUNK_MAX_ENTRIES
}

fn sorted_run_catchup_chunk_max_bytes_default() -> usize {
    SORTED_RUN_CATCHUP_CHUNK_MAX_BYTES
}

fn compaction_scheduler_active() -> bool {
    true
}

fn compaction_scheduler_max_debt_bytes_default() -> u64 {
    COMPACTION_SCHEDULER_MAX_DEBT_BYTES
}

fn blob_value_threshold_bytes_default() -> usize {
    BLOB_VALUE_THRESHOLD_BYTES
}

fn blob_gc_active() -> bool {
    true
}

const MEMTABLE_GC_ENABLED: bool = false;
const MEMTABLE_GC_OPS_INTERVAL: u64 = 1000;
const LSM_STATS_REFRESH_OPS_INTERVAL: u64 = DEFAULT_LSM_STATS_REFRESH_OPS_INTERVAL;
const BLOB_GC_OPS_INTERVAL: u64 = DEFAULT_BLOB_GC_OPS_INTERVAL;
const AUTOPILOT_HOTMETA_MAX_WRITE_OPS_PER_TICK: u64 = 50_000;
const AUTOPILOT_TIERING_MIN_LIVE_BYTES: u64 = 0;
const AUTOPILOT_TIERING_MAX_LIVE_BYTES: u64 = 512 * 1024 * 1024;
const RAFT_PERSIST_INTERVAL_OPS: u32 = DEFAULT_RAFT_PERSIST_INTERVAL_OPS;
const CLOCK_PERSIST_INTERVAL_OPS: u32 = DEFAULT_CLOCK_PERSIST_INTERVAL_OPS;
const WAL_COMPLETION_TIMEOUT: Duration = Duration::from_millis(DEFAULT_WAL_COMPLETION_TIMEOUT_MS);
const APPLY_LANE_BATCH_MAX: usize = DEFAULT_APPLY_LANE_BATCH_MAX;
const REPLICATION_MAX_IN_FLIGHT: usize = DEFAULT_REPLICATION_MAX_IN_FLIGHT;
const REPLICATION_MAX_TARGETS: usize = DEFAULT_REPLICATION_MAX_TARGETS;
const REPLICATION_HEDGE_EXTRA: usize = DEFAULT_REPLICATION_HEDGE_EXTRA;
const REPLICATION_DYNAMIC_HEDGE: bool = true;
const WRITE_FLUSH_WINDOW: Duration = DEFAULT_WRITE_FLUSH_WINDOW;
const WRITE_FLUSH_MAX_OPS: usize = DEFAULT_WRITE_FLUSH_MAX_OPS;
const WRITE_FLUSH_SOFT_BYTES: usize = DEFAULT_WRITE_FLUSH_SOFT_BYTES;

fn insert_fast_lane_active() -> bool {
    true
}

fn latency_frontier_mode_active() -> bool {
    true
}

fn simulation_replication_fallback_allowed() -> bool {
    true
}

pub mod abi;
pub mod admin_api;
pub mod analytics;
pub mod api;
pub mod audit;
pub mod autopilot;
pub mod backup;
pub mod cdc;
pub mod checkpoint;
pub mod cluster;
pub mod codec;
pub mod config;
pub mod control_plane;
pub mod coord;
pub mod drill;
pub mod erasure;
pub mod failover;
pub mod gateway;
pub mod hlc;
pub mod invariant_history;
pub mod keyspace;
pub mod lsm;
pub mod mvcc;
pub mod net;
pub mod placement;
pub mod planner;
pub mod quorum;
pub mod raft;
pub mod read;
pub mod replication;
pub mod restore;
pub mod routing;
pub mod rpc;
pub mod schema_evolution;
pub mod schema_gate;
pub mod scrub;
pub mod security;
pub mod shard;
pub mod snapshot;
pub mod sql;
pub mod tenant;
pub mod time;
pub mod topology;
pub mod txn;
pub mod types;
pub mod versioning;
pub mod wal;
pub mod writer;

#[derive(Debug)]
pub struct DbEngine {
    memtable: Memtable,
    blob_store: BlobStore,
    read_path: ReadPath,
    wal: Arc<WalSegment>,
    lane_wals: Vec<Arc<WalSegment>>,
    replication_groups: HashMap<u32, ReplicationState>,
    raft_current_term: u64,
    raft_last_log_index: u64,
    raft_last_committed_index: u64,
    raft_persist_interval_ops: u32,
    raft_persist_ops_since_flush: u32,
    clock_persist_interval_ops: u32,
    clock_persist_ops_since_flush: u32,
    #[cfg(test)]
    pending_append_responses: Vec<FollowerAppendResponse>,
    clock: HybridLogicalClock,
    next_txn_id: u64,
    txns: HashMap<u64, TxnRecord>,
    lock_table: TxnLockTable,
    cdc: CdcEmitter,
    cdc_checkpoints: crate::db::cdc::CdcCheckpointStore,
    next_snapshot_id: u64,
    snapshots: HashMap<u64, SnapshotRecord>,
    wal_path: PathBuf,
    uncertainty: UncertaintyTracker,
    watermarks: SafeReadWatermarks,
    safe_time: SafeTimePropagator,
    clock_persist_error: Option<String>,
    clock_persist_error_at: Option<u64>,
    raft_persist_error: Option<String>,
    raft_persist_error_at: Option<u64>,
    cdc_checkpoint_persist_error: Option<String>,
    cdc_checkpoint_persist_error_at: Option<u64>,
    checkpoint_persist_error: Option<String>,
    checkpoint_persist_error_at: Option<u64>,
    checkpoint_restore_error: Option<String>,
    checkpoint_restore_error_at: Option<u64>,
    schema_gate_error: Option<String>,
    schema_gate_error_at: Option<u64>,
    topology_state_dirty: bool,
    schema_committed_epoch: u64,
    schema_all_voters_on_target_binary: bool,
    checkpoint_interval_secs: u64,
    checkpoint_last_epoch_s: u64,
    checkpoint_config: crate::db::checkpoint::CheckpointConfig,
    checkpoint_manager: Option<crate::db::checkpoint::CheckpointManager>,
    local_region: String,
    topology_region_az_node_map: RegionAzNodeMap,
    topology_canonical_regions: BTreeSet<String>,
    checkpoint_allowed_regions: Vec<String>,
    sovereignty_id: String,
    sovereignty_allowed_regions: Vec<String>,
    sovereignty_enforce_all_copies: bool,
    replication_async_failover: bool,
    residency_policy: Option<ResidencyPolicy>,
    shard_directory: ShardDirectory,
    home_store: crate::db::placement::PlacementHomeStore,
    keyrange_ownership: BTreeMap<String, KeyrangeOwnershipState>,
    replication_factor: u32,
    write_quorum: u32,
    quorum_transport_mode: QuorumTransportMode,
    commit_visibility_mode: CommitVisibilityMode,
    replicated_log_backend: ReplicatedLogBackend,
    autoscale_enabled: bool,
    autoscale_mode: AutoscaleMode,
    autoscale_tick_ms: u64,
    autoscale_max_skew_ratio: f64,
    autoscale_target_shards_per_group: u32,
    autoscale_max_active_groups: u32,
    autoscale_max_logical_shards: u32,
    autoscale_last_tick_epoch_ms: u64,
    autoscale_status: DbAutoscaleStatus,
    intent_config: crate::db::autopilot::compiler::DbIntentConfig,
    autopilot_action_seq: u64,
    autopilot_intent_effective: DbIntentEffective,
    autopilot_intent_conflicts: Vec<DbIntentConflict>,
    autopilot_tiering_state: DbTieringState,
    autopilot_recommendations: Vec<DbRecommendation>,
    autopilot_audit_ring: crate::db::autopilot::orchestrator::AuditRingBuffer,
    shard_write_ops: HashMap<u32, u64>,
    /// Accumulates shard op counts from writer lanes outside the engine write lock.
    /// Drained into shard_write_ops before the autoscale tick.
    shard_write_ops_accum: Arc<std::sync::Mutex<HashMap<u32, u64>>>,
    write_stage: WriteStageTelemetry,
    client_write_path: ClientWritePathTelemetry,
    replication_telemetry: ReplicationTelemetry,
    replicated_log_telemetry: ReplicatedLogTelemetry,
    jupiter_telemetry: JupiterFeatureTelemetry,
    sorted_run_installs: BTreeMap<u64, SortedRunInstallState>,
    sorted_run_progress: HashMap<u64, u64>,
    apply_backlog_peak: u64,
    lsm_cached_stats: MemtableStats,
    lsm_stats_dirty: bool,
    lsm_stats_ops_since_refresh: u64,
    lsm_stats_refresh_ops_interval: u64,
    blob_gc_ops_since_run: u64,
    blob_gc_ops_interval: u64,
    memtable_gc_write_counter: u64,
    #[cfg(test)]
    autoscale_test_healthy_nodes: Option<Vec<u64>>,
    #[cfg(test)]
    fail_next_cdc_checkpoint_persist: bool,
}

const PRIMARY_ACTIVE_GROUP_ID: u32 = 0;

#[derive(Debug, Clone, PartialEq, Eq)]
struct KeyrangeOwnershipState {
    keyrange_id: String,
    sovereignty_id: String,
    home_region: String,
    home_epoch: u64,
    leader_node_id: String,
    ownership_token: String,
    async_failover_regions: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnershipFence {
    pub expected_home_epoch: u64,
    pub expected_shard_map_epoch: u64,
    pub ownership_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerRecord {
    pub keyrange_id: String,
    pub sovereignty_id: String,
    pub home_region: String,
    pub home_epoch: u64,
    pub leader_node_id: String,
    pub ownership_token: String,
    pub shard_map_epoch: u64,
    pub async_failover_regions: Vec<String>,
}

#[derive(Debug, Clone)]
struct ReplicationState {
    leader: NodeState,
    followers: HashMap<u64, NodeState>,
    membership: MembershipConfig,
    durability_commit_index: u64,
    apply_visible_index: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadConsistency {
    Strong,
    Eventual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitVisibilityMode {
    /// Legacy behavior: apply to local state before user-visible success.
    StrictApply,
    /// Moonshot mode: ACK after durable quorum log commit, apply asynchronously.
    AsyncApply,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuorumTransportMode {
    PreferPrivateRpc,
    RequirePrivateRpc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplicatedLogBackend {
    DualWal,
    ShadowCanonical,
    CanonicalOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbReplicaAckStatus {
    pub node_id: u64,
    pub durable_ack: bool,
    pub response_success: bool,
    pub response_term: u64,
    pub response_match_index: u64,
    pub response_conflict_index: Option<u64>,
    pub replication_latency_ns: u64,
    pub fsync_latency_ns: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbWriterLaneStatus {
    pub lane_id: usize,
    pub assigned_shards: u64,
    pub queue_depth: u64,
    pub enqueue_attempts: u64,
    pub enqueue_rejections: u64,
    pub depth_samples: u64,
    pub saturated_samples: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbApplyLaneStatus {
    pub lane_id: usize,
    pub queue_depth: u64,
    pub enqueue_attempts: u64,
    pub depth_samples: u64,
    pub max_queue_depth: u64,
    pub dequeued_tasks: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbFailureCounter {
    pub token: String,
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbHealthStatus {
    pub clock_persist_error: Option<String>,
    pub clock_persist_error_at: Option<u64>,
    pub raft_persist_error: Option<String>,
    pub raft_persist_error_at: Option<u64>,
    pub cdc_checkpoint_persist_error: Option<String>,
    pub cdc_checkpoint_persist_error_at: Option<u64>,
    pub checkpoint_persist_error: Option<String>,
    pub checkpoint_persist_error_at: Option<u64>,
    pub checkpoint_restore_error: Option<String>,
    pub checkpoint_restore_error_at: Option<u64>,
    pub schema_gate_error: Option<String>,
    pub schema_gate_error_at: Option<u64>,
    pub replication_queue_depth: u64,
    pub replication_queue_depth_peak: u64,
    pub replication_batch_samples: u64,
    pub replication_batch_ops_le_1: u64,
    pub replication_batch_ops_le_4: u64,
    pub replication_batch_ops_le_16: u64,
    pub replication_batch_ops_le_64: u64,
    pub replication_batch_ops_gt_64: u64,
    pub replication_batch_bytes_le_1k: u64,
    pub replication_batch_bytes_le_4k: u64,
    pub replication_batch_bytes_le_16k: u64,
    pub replication_batch_bytes_le_64k: u64,
    pub replication_batch_bytes_gt_64k: u64,
    pub quorum_ack_count: u64,
    pub quorum_size: u64,
    pub quorum_replication_latency_ns: u64,
    pub quorum_fsync_latency_ns: u64,
    pub quorum_failure_token: Option<String>,
    pub quorum_failure_reason: Option<String>,
    pub replica_acks: Vec<DbReplicaAckStatus>,
    pub replication_target_count: u64,
    pub replication_contacted_count: u64,
    pub replication_wave_count: u64,
    pub replication_wave_avg_targets: u64,
    pub replication_wave_max_targets: u64,
    pub replication_successful_count: u64,
    pub replication_failed_count: u64,
    pub replication_cancelled_count: u64,
    pub replication_contact_efficiency_bps: u64,
    pub replication_target_efficiency_bps: u64,
    pub replication_skipped_count: u64,
    pub replication_aborted_in_flight_count: u64,
    pub replication_failure_counters: Vec<DbFailureCounter>,
    pub replication_simulation_commits: u64,
    pub replication_rpc_max_in_flight: u64,
    pub replication_rpc_in_flight: u64,
    pub replication_rpc_available_permits: u64,
    pub replication_rpc_backpressure_timeouts: u64,
    pub replication_rpc_backpressure_closed: u64,
    pub quorum_transport_mode: QuorumTransportMode,
    pub writer_lanes: Vec<DbWriterLaneStatus>,
    pub writer_lane_max_enqueue_share_bps: u64,
    pub writer_lane_max_retry_after_bps: u64,
    pub writer_lane_max_saturation_bps: u64,
    pub writer_lane_assignment_lookups: u64,
    pub writer_lane_assignment_hits: u64,
    pub writer_lane_assignment_misses: u64,
    pub writer_lane_assignment_hit_rate_bps: u64,
    pub apply_lanes: Vec<DbApplyLaneStatus>,
    pub apply_lane_max_queue_depth: u64,
    pub replicated_log_backend: ReplicatedLogBackend,
    pub replicated_log_shadow_payload_bytes: u64,
    pub replicated_log_shadow_wal_bytes: u64,
    pub replicated_log_shadow_overhead_bytes: u64,
    pub apply_backlog_depth: u64,
    pub apply_backlog_peak: u64,
    pub lsm_compaction_debt_bytes_estimate: u64,
    pub lsm_shadow_bytes_estimate: u64,
    pub lsm_live_bytes_estimate: u64,
    pub lsm_total_bytes_estimate: u64,
    pub lsm_version_count: u64,
    pub lsm_tombstone_count: u64,
    pub replication_outside_lock_active: bool,
    pub wal_encode_outside_lock_active: bool,
    pub sorted_run_catchup_active: bool,
    pub sorted_run_catchup_lag_threshold_ops: u64,
    pub sorted_run_catchup_requests: u64,
    pub sorted_run_catchup_chunks_sent: u64,
    pub sorted_run_catchup_chunks_applied: u64,
    pub compaction_scheduler_active: bool,
    pub compaction_scheduler_max_debt_bytes: u64,
    pub compaction_scheduler_admitted: u64,
    pub compaction_scheduler_deferred: u64,
    pub compaction_scheduler_rejected: u64,
    pub blob_value_threshold_bytes: u64,
    pub blob_gc_active: bool,
    pub blob_values_externalized: u64,
    pub blob_gc_runs: u64,
    pub blob_gc_reclaimed_bytes: u64,
    pub insert_fast_lane_active: bool,
    pub insert_fast_lane_accepted: u64,
    pub insert_fast_lane_rejected: u64,
    pub latency_frontier_mode_active: bool,
    pub frontier_speculative_plans: u64,
    pub frontier_wave_plans: u64,
    pub memtable_gc_enabled: bool,
    pub memtable_gc_runs: u64,
    pub memtable_gc_versions_dropped: u64,
    pub memtable_gc_tombstone_keys_removed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortedRunCatchUpChunkInstallStatus {
    pub accepted: bool,
    pub next_chunk_index: u64,
    pub rejection_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbPrivateMeshStatus {
    pub mesh_ready: bool,
    pub reason: String,
    pub machine_id: String,
    pub leader_id: String,
    pub node_count: usize,
    pub min_ready_nodes: usize,
    pub nodes: Vec<String>,
    pub last_refresh_epoch_ms: u64,
}

pub use config::DbConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoscaleMode {
    GrowOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbAutoscaleStatus {
    pub enabled: bool,
    pub mode: AutoscaleMode,
    pub last_action: String,
    pub reasons: Vec<String>,
    pub cooldown_ms: u64,
    pub last_action_at_epoch_ms: u64,
}

pub type DbIntentEffective = crate::db::autopilot::orchestrator::IntentEffective;
pub type DbIntentConflict = crate::db::autopilot::orchestrator::IntentConflict;
pub type DbAutopilotAuditRow = crate::db::autopilot::orchestrator::AutopilotAuditRow;
pub type DbTieringState = crate::db::autopilot::orchestrator::TieringState;
pub type DbRecommendation = crate::db::autopilot::orchestrator::Recommendation;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbGroupTopologyStatus {
    pub group_id: u32,
    pub voters: Vec<u64>,
    pub learners: Vec<u64>,
    pub current_term: u64,
    pub last_log_index: u64,
    pub commit_index: u64,
    pub durability_commit_index: u64,
    pub apply_visible_index: u64,
    pub apply_backlog_depth: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbTopologyStatus {
    pub logical_shards: u32,
    pub active_groups: u32,
    pub shard_map_epoch: u64,
    pub replication_factor: u32,
    pub write_quorum: u32,
    pub groups: Vec<DbGroupTopologyStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DbCommitVisibilityStatus {
    pub mode: CommitVisibilityMode,
    pub durability_commit_index: u64,
    pub apply_visible_index: u64,
    pub apply_backlog_depth: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TxnState {
    Active,
    Prepared,
    Committed,
    Aborted,
}

#[derive(Debug, Clone, Copy)]
struct TxnRecord {
    state: TxnState,
    start_ts: u64,
    prepared_ts: Option<u64>,
    commit_ts: Option<u64>,
    bound_shard: Option<u32>,
}

#[derive(Debug, Clone)]
struct SnapshotRecord {
    created_ts: u64,
    progress: u8,
    restored_ts: Option<u64>,
    checkpoint_id: String,
}

#[derive(Debug, Clone)]
enum StagedApplyOp {
    Put {
        user_key: EncodedUserKey,
        namespace: Bytes,
        key: Bytes,
        value: Bytes,
        version: u64,
    },
    Delete {
        user_key: EncodedUserKey,
        namespace: Bytes,
        key: Bytes,
        version: u64,
    },
}

/// Pre-computed batch data built outside the DbEngine lock to minimize
/// critical-section time. Contains validated frame, command payloads,
/// and batch weight metrics.
struct PreProcessedBatch {
    frame: RaftAppendFrame,
    command_payloads: Vec<Vec<u8>>,
    op_count: usize,
    byte_count: usize,
}

fn preprocess_batch(batch: &[BatchOp]) -> Result<PreProcessedBatch, DbError> {
    DbEngine::validate_batch(batch)?;
    let frame = build_append_frame(batch);
    let command_payloads: Vec<Vec<u8>> = frame.commands.iter().map(command_payload).collect();
    let (op_count, byte_count) = envelope_batch_weight(batch);
    Ok(PreProcessedBatch {
        frame,
        command_payloads,
        op_count,
        byte_count,
    })
}

thread_local! {
    /// Reusable buffer for WAL encoding to avoid allocating a Vec per batch.
    static WAL_ENCODE_BUF: std::cell::RefCell<Vec<u8>> = std::cell::RefCell::new(Vec::new());
    /// Reusable HashMap for OCC shadow-version tracking. Cleared (not dropped) between batches so
    /// the allocated bucket array survives across calls on the same writer-lane thread.
    static SHADOW_VERSIONS_BUF: std::cell::RefCell<HashMap<EncodedUserKey, Option<u64>>> =
        std::cell::RefCell::new(HashMap::new());
}

/// Encodes records into a thread-local buffer and invokes `f` with the slice and encode timing.
/// Caller can pass the slice to WAL append (e.g. `append_raw_bytes_with_metrics_slice`) to avoid
/// a per-batch Vec allocation in the encode path.
fn encode_records_to_wal_bytes<R, F>(records: &[Record], f: F) -> R
where
    F: FnOnce(&[u8], u64) -> R,
{
    let encode_started = Instant::now();
    let estimated_len: usize = records
        .iter()
        .map(|record| HEADER_BYTES + record.namespace.len() + record.key.len() + record.value.len())
        .sum();
    WAL_ENCODE_BUF.with(|cell| {
        let mut buf = cell.borrow_mut();
        buf.clear();
        buf.reserve(estimated_len);
        for record in records {
            encode_to(record, &mut buf);
        }
        let encode_ns = duration_to_nanos(encode_started.elapsed());
        f(&buf, encode_ns)
    })
}

const BLOB_REF_VALUE_SENTINEL: &[u8] = b"\0WRELA_BLOB_REF\0";

fn encode_blob_ref_value(blob_id: u64, len_bytes: u32) -> Bytes {
    let mut out = Vec::with_capacity(BLOB_REF_VALUE_SENTINEL.len() + 8 + 4);
    out.extend_from_slice(BLOB_REF_VALUE_SENTINEL);
    out.extend_from_slice(&blob_id.to_be_bytes());
    out.extend_from_slice(&len_bytes.to_be_bytes());
    Bytes::from(out)
}

fn decode_blob_ref_value(value: &[u8]) -> Option<(u64, u32)> {
    if !value.starts_with(BLOB_REF_VALUE_SENTINEL) {
        return None;
    }
    let expected_len = BLOB_REF_VALUE_SENTINEL.len() + 8 + 4;
    if value.len() != expected_len {
        return None;
    }
    let blob_id_offset = BLOB_REF_VALUE_SENTINEL.len();
    let len_offset = blob_id_offset + 8;
    let blob_id = u64::from_be_bytes(value.get(blob_id_offset..len_offset)?.try_into().ok()?);
    let len_bytes = u32::from_be_bytes(value.get(len_offset..expected_len)?.try_into().ok()?);
    Some((blob_id, len_bytes))
}

fn externalize_value_for_memtable(blob_store: &mut BlobStore, value: Bytes) -> (Bytes, bool) {
    match blob_store.separate_value(value.to_vec(), blob_value_threshold_bytes_default()) {
        ValuePlacement::Inline(value) => (Bytes::from(value), false),
        ValuePlacement::BlobRef { blob_id, len_bytes } => {
            (encode_blob_ref_value(blob_id, len_bytes), true)
        }
    }
}

fn materialize_value_from_memtable(blob_store: &BlobStore, value: &[u8]) -> Option<Vec<u8>> {
    if let Some((blob_id, len_bytes)) = decode_blob_ref_value(value) {
        return blob_store.read(&ValuePlacement::BlobRef { blob_id, len_bytes });
    }
    Some(value.to_vec())
}

/// Persist work captured under the DbEngine lock but executed after
/// lock release. This moves fsync I/O off the critical mutex path.
struct DeferredPersistWork {
    raft_metadata: Option<(PathBuf, RaftPersistMetadata)>,
    clock_packed: Option<(PathBuf, u64)>,
    topology_state: Option<(PathBuf, PersistedTopologyState)>,
}

/// Result of prepare_and_apply_batch. WAL bytes are submitted outside the lock;
/// stage data is used to record telemetry after WAL sync completes.
struct PrepareAndApplyResult {
    active_group_id: u32,
    required_index: u64,
    committed_versions: Vec<u64>,
    staged_ops: Vec<StagedApplyOp>,
    deferred: DeferredPersistWork,
    wal_records: Vec<Record>,
    wal_bytes: Vec<u8>,
    wal_ops: usize,
    encode_ns: u64,
    stage_data: WriteStagePartialData,
    /// Shard ID and op count for updating shard_write_ops outside the engine write lock.
    shard_ops_delta: Option<(u32, u64)>,
}

/// Batch state prepared under the DbEngine lock for the replication-outside-lock path.
/// Writer lanes execute network fanout using this payload after releasing the mutex,
/// then call finalize_prepared_batch_after_outside_replication under lock.
struct PreparedOutsideLockBatch {
    active_group_id: u32,
    required_term: u64,
    required_index: u64,
    logical_shard_id: u32,
    ownership_fence: OwnershipFence,
    batch_ops: Vec<BatchOp>,
    membership: MembershipConfig,
    write_quorum_required: usize,
    replica_latency_rank: BTreeMap<u64, u64>,
    follower_progress_hints: BTreeMap<u64, u64>,
    leader_commit: u64,
    leader_snapshot: NodeState,
    follower_snapshots: Vec<(u64, NodeState)>,
    require_private_rpc_transport: bool,
    simulation_fallback_allowed: bool,
    committed_versions: Vec<u64>,
    staged_records: Vec<Record>,
    staged_ops: Vec<StagedApplyOp>,
    staged_entries: Vec<LogEntry>,
    max_version: u64,
    op_count: u64,
    byte_count: u64,
    queue_wait_ns: u64,
    engine_lock_wait_ns: u64,
    validate_route_ns: u64,
    total_started: Instant,
}

struct OutsideLockFanoutResult {
    replicate_ns: u64,
    used_simulation: bool,
    sorted_run_chunks_sent: u64,
    total_target_count: usize,
    contacted_target_count: usize,
    replication_wave_count: usize,
    replication_wave_total_targets: usize,
    replication_wave_max_targets: usize,
    successful_target_count: usize,
    failed_target_count: usize,
    cancelled_target_count: usize,
    aborted_in_flight_count: usize,
    follower_responses: Vec<FollowerAppendResponse>,
    follower_state_updates: Vec<(u64, NodeState)>,
    follower_progress_updates: Vec<(u64, u64)>,
    replication_error: Option<DbError>,
}

struct OutsideLockReplicationError {
    token: &'static str,
    detail: String,
    total_target_count: usize,
    contacted_target_count: usize,
    replication_wave_count: usize,
    replication_wave_total_targets: usize,
    replication_wave_max_targets: usize,
    successful_target_count: usize,
    failed_target_count: usize,
    cancelled_target_count: usize,
    aborted_in_flight_count: usize,
    replication_error: Option<DbError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplicationCommitMode {
    Quorum,
    ReplicaLocal,
}

#[derive(Clone)]
struct WriteStagePartialData {
    op_count: u64,
    byte_count: u64,
    queue_wait_ns: u64,
    engine_lock_wait_ns: u64,
    validate_route_ns: u64,
    replicate_ns: u64,
    apply_ns: u64,
    raft_persist_ns: u64,
    clock_persist_ns: u64,
    total_started: Instant,
}

impl DeferredPersistWork {
    fn execute(self) {
        let ignore_missing_path = |message: &str| {
            message.contains("No such file or directory") || message.contains("os error 2")
        };
        let rt = runtime::tokio_runtime();
        rt.block_on(async {
            let raft_meta_work = self.raft_metadata.map(|(wal_path, metadata)| {
                tokio::task::spawn_blocking(move || {
                    persist_raft_metadata_binary(&wal_path, &metadata)
                })
            });
            let clock_work = self.clock_packed.map(|(wal_path, packed)| {
                tokio::task::spawn_blocking(move || persist_hlc_state(&wal_path, packed))
            });
            let topology_work = self.topology_state.map(|(wal_path, state)| {
                tokio::task::spawn_blocking(move || persist_topology_state(&wal_path, &state))
            });
            if let Some(h) = raft_meta_work {
                if let Ok(Err(err)) = h.await {
                    if !ignore_missing_path(&err.to_string()) {
                        eprintln!("deferred raft metadata persist failed: {err}");
                    }
                }
            }
            if let Some(h) = clock_work {
                if let Ok(Err(err)) = h.await {
                    if !ignore_missing_path(&err.to_string()) {
                        eprintln!("deferred clock persist failed: {err}");
                    }
                }
            }
            if let Some(h) = topology_work {
                if let Ok(Err(err)) = h.await {
                    if !ignore_missing_path(&err.to_string()) {
                        eprintln!("deferred topology persist failed: {err}");
                    }
                }
            }
        });
    }
}

#[derive(Debug, Clone, Default)]
struct ReplicationTelemetry {
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
    last_quorum_acks: u64,
    last_quorum_size: u64,
    last_quorum_replication_latency_ns: u64,
    last_quorum_fsync_latency_ns: u64,
    last_failure_token: Option<String>,
    last_failure_reason: Option<String>,
    last_replica_acks: Vec<DbReplicaAckStatus>,
    last_target_count: u64,
    last_contacted_count: u64,
    last_wave_count: u64,
    last_wave_avg_targets: u64,
    last_wave_max_targets: u64,
    last_successful_count: u64,
    last_failed_count: u64,
    last_cancelled_count: u64,
    last_skipped_count: u64,
    last_aborted_in_flight_count: u64,
    simulation_commits: u64,
    failure_counters: BTreeMap<String, u64>,
    replica_latency_ewma_ns: BTreeMap<u64, u64>,
}

#[derive(Debug, Clone, Default)]
struct ReplicatedLogTelemetry {
    observed_batches: u64,
    payload_bytes: u64,
    wal_bytes: u64,
}

#[derive(Debug, Clone, Default)]
struct JupiterFeatureTelemetry {
    sorted_run_catchup_requests: u64,
    sorted_run_catchup_chunks_sent: u64,
    sorted_run_catchup_chunks_applied: u64,
    compaction_scheduler_admitted: u64,
    compaction_scheduler_deferred: u64,
    compaction_scheduler_rejected: u64,
    blob_values_externalized: u64,
    blob_gc_runs: u64,
    blob_gc_reclaimed_bytes: u64,
    insert_fast_lane_accepted: u64,
    insert_fast_lane_rejected: u64,
    frontier_speculative_plans: u64,
    frontier_wave_plans: u64,
    memtable_gc_runs: u64,
    memtable_gc_versions_dropped: u64,
    memtable_gc_tombstone_keys_removed: u64,
}

#[derive(Debug, Clone, Default)]
struct SortedRunInstallState {
    term: u64,
    total_chunks: u64,
    next_chunk_index: u64,
    chunk_payloads: BTreeMap<u64, Vec<u8>>,
    chunk_hashes: BTreeMap<u64, u64>,
}

impl SortedRunInstallState {
    fn new(term: u64, total_chunks: u64) -> Self {
        Self {
            term,
            total_chunks,
            next_chunk_index: 0,
            chunk_payloads: BTreeMap::new(),
            chunk_hashes: BTreeMap::new(),
        }
    }
}

impl JupiterFeatureTelemetry {
    fn record_insert_fast_lane_attempt(&mut self, accepted: bool) {
        if accepted {
            self.insert_fast_lane_accepted = self.insert_fast_lane_accepted.saturating_add(1);
        } else {
            self.insert_fast_lane_rejected = self.insert_fast_lane_rejected.saturating_add(1);
        }
    }
}

impl ReplicationTelemetry {
    fn observe_batch(&mut self, ops: usize, bytes: usize) {
        let ops = ops as u64;
        let bytes = bytes as u64;
        self.batch_samples = self.batch_samples.saturating_add(1);
        if ops <= 1 {
            self.batch_ops_le_1 = self.batch_ops_le_1.saturating_add(1);
        } else if ops <= 4 {
            self.batch_ops_le_4 = self.batch_ops_le_4.saturating_add(1);
        } else if ops <= 16 {
            self.batch_ops_le_16 = self.batch_ops_le_16.saturating_add(1);
        } else if ops <= 64 {
            self.batch_ops_le_64 = self.batch_ops_le_64.saturating_add(1);
        } else {
            self.batch_ops_gt_64 = self.batch_ops_gt_64.saturating_add(1);
        }
        if bytes <= 1024 {
            self.batch_bytes_le_1k = self.batch_bytes_le_1k.saturating_add(1);
        } else if bytes <= 4096 {
            self.batch_bytes_le_4k = self.batch_bytes_le_4k.saturating_add(1);
        } else if bytes <= 16 * 1024 {
            self.batch_bytes_le_16k = self.batch_bytes_le_16k.saturating_add(1);
        } else if bytes <= 64 * 1024 {
            self.batch_bytes_le_64k = self.batch_bytes_le_64k.saturating_add(1);
        } else {
            self.batch_bytes_gt_64k = self.batch_bytes_gt_64k.saturating_add(1);
        }
    }

    fn set_queue_depth(&mut self, depth: u64) {
        self.queue_depth = depth;
        self.queue_depth_peak = self.queue_depth_peak.max(depth);
    }

    fn clear_queue_depth(&mut self) {
        self.queue_depth = 0;
    }

    fn record_ack_decision(
        &mut self,
        durable_acks: usize,
        quorum_size: usize,
        replication_latency_ns: u64,
        fsync_latency_ns: u64,
        required_term: u64,
        required_index: u64,
        follower_responses: &[FollowerAppendResponse],
    ) {
        self.last_quorum_acks = durable_acks as u64;
        self.last_quorum_size = quorum_size as u64;
        self.last_quorum_replication_latency_ns = replication_latency_ns;
        self.last_quorum_fsync_latency_ns = fsync_latency_ns;
        self.last_replica_acks = follower_responses
            .iter()
            .map(|follower| DbReplicaAckStatus {
                node_id: follower.node_id,
                durable_ack: response_is_durable_ack(
                    &follower.response,
                    required_term,
                    required_index,
                ),
                response_success: follower.response.success,
                response_term: follower.response.term,
                response_match_index: follower.response.match_index,
                response_conflict_index: follower.response.conflict_index,
                replication_latency_ns: follower.replication_latency_ns,
                fsync_latency_ns: follower.fsync_latency_ns,
            })
            .collect();
        for follower in follower_responses {
            let observed = follower.replication_latency_ns.max(1);
            let entry = self
                .replica_latency_ewma_ns
                .entry(follower.node_id)
                .or_insert(observed);
            // Conservative EWMA to avoid overreacting to one lucky sample.
            *entry = ((*entry).saturating_mul(7) / 8).saturating_add(observed / 8);
        }
        self.last_replica_acks
            .sort_unstable_by_key(|sample| sample.node_id);
        self.last_failure_token = None;
        self.last_failure_reason = None;
    }

    fn replica_priority_rank(&self, node_id: u64) -> u64 {
        self.replica_latency_ewma_ns
            .get(&node_id)
            .copied()
            .unwrap_or(u64::MAX / 2)
    }

    fn record_fanout_shape(
        &mut self,
        target_count: usize,
        contacted_count: usize,
        wave_count: usize,
        wave_avg_targets: usize,
        wave_max_targets: usize,
        successful_count: usize,
        failed_count: usize,
        cancelled_count: usize,
        skipped_count: usize,
        aborted_in_flight_count: usize,
    ) {
        self.last_target_count = target_count as u64;
        self.last_contacted_count = contacted_count as u64;
        self.last_wave_count = wave_count as u64;
        self.last_wave_avg_targets = wave_avg_targets as u64;
        self.last_wave_max_targets = wave_max_targets as u64;
        self.last_successful_count = successful_count as u64;
        self.last_failed_count = failed_count as u64;
        self.last_cancelled_count = cancelled_count as u64;
        self.last_skipped_count = skipped_count as u64;
        self.last_aborted_in_flight_count = aborted_in_flight_count as u64;
    }

    fn record_failure(
        &mut self,
        token: impl Into<String>,
        reason: impl Into<String>,
        preserve_replica_acks: bool,
    ) {
        let token = token.into();
        self.last_failure_token = Some(token.clone());
        self.last_failure_reason = Some(reason.into());
        let entry = self.failure_counters.entry(token).or_insert(0);
        *entry = entry.saturating_add(1);
        if !preserve_replica_acks {
            self.last_replica_acks.clear();
        }
    }

    fn increment_failure_counter(&mut self, token: impl Into<String>) {
        let entry = self.failure_counters.entry(token.into()).or_insert(0);
        *entry = entry.saturating_add(1);
    }
}

impl ReplicatedLogTelemetry {
    fn observe_batch(&mut self, payload_bytes: usize, wal_bytes: usize) {
        self.observed_batches = self.observed_batches.saturating_add(1);
        self.payload_bytes = self.payload_bytes.saturating_add(payload_bytes as u64);
        self.wal_bytes = self.wal_bytes.saturating_add(wal_bytes as u64);
    }

    fn overhead_bytes(&self) -> u64 {
        self.wal_bytes.saturating_sub(self.payload_bytes)
    }
}

const DEFAULT_POINT_READ_IN_FLIGHT_LIMIT: usize = 64;
const DEFAULT_RANGE_READ_IN_FLIGHT_LIMIT: usize = 8;
const DEFAULT_POINT_READ_CACHE_CAPACITY: usize = 1024;
const DEFAULT_NEGATIVE_BLOOM_CAPACITY: usize = 1024;
const LOCAL_NODE_ID: u64 = 1;
const DEFAULT_MAX_CLOCK_SKEW_MS: u64 = 25;
const LOCAL_REGION_ID: &str = "local";
const DEFAULT_WRITE_FLUSH_WINDOW: Duration = Duration::from_micros(500);
const DEFAULT_WRITE_FLUSH_MAX_OPS: usize = 256;
const DEFAULT_WRITE_FLUSH_SOFT_BYTES: usize = 512 * 1024;
const DEFAULT_APPLY_LANE_BATCH_MAX: usize = 64;
const DEFAULT_RAFT_PERSIST_INTERVAL_OPS: u32 = 64;
const DEFAULT_CLOCK_PERSIST_INTERVAL_OPS: u32 = 16;
const WRITE_LANE_QUEUE_MULTIPLIER: usize = 8;
const WRITE_LANE_SATURATION_PCT: usize = 80;
const DEFAULT_WAL_COMPLETION_TIMEOUT_MS: u64 = 2_000;
const DEFAULT_REPLICATION_MAX_IN_FLIGHT: usize = 32;
const DEFAULT_REPLICATION_MAX_TARGETS: usize = 256;
const DEFAULT_REPLICATION_HEDGE_EXTRA: usize = 1;
const QUORUM_FAILURE_TOKEN_PRIVATE_MESH_NOT_READY: &str = "QUORUM_PRIVATE_MESH_NOT_READY";
const QUORUM_FAILURE_TOKEN_PRIVATE_RPC_REQUIRED: &str = "QUORUM_PRIVATE_RPC_REQUIRED";
const QUORUM_FAILURE_TOKEN_TARGET_SET_TOO_SMALL: &str = "QUORUM_TARGET_SET_TOO_SMALL";
const QUORUM_FAILURE_TOKEN_DURABILITY_NOT_REACHED: &str = "QUORUM_DURABILITY_NOT_REACHED";
const QUORUM_FAILURE_TOKEN_REPLICATION_IN_FLIGHT_LIMIT: &str = "QUORUM_REPLICATION_IN_FLIGHT_LIMIT";
const QUORUM_FAILURE_TOKEN_REPLICATION_RPC_BACKPRESSURE: &str =
    "QUORUM_REPLICATION_RPC_BACKPRESSURE";
const QUORUM_FAILURE_TOKEN_PRIVATE_RPC_UNAVAILABLE: &str = "QUORUM_PRIVATE_RPC_UNAVAILABLE";
const QUORUM_FAILURE_TOKEN_PRIVATE_RPC_RETRY_AFTER: &str = "QUORUM_PRIVATE_RPC_RETRY_AFTER";
const QUORUM_FAILURE_TOKEN_PRIVATE_RPC_NOT_LEADER: &str = "QUORUM_PRIVATE_RPC_NOT_LEADER";
const QUORUM_FAILURE_TOKEN_PRIVATE_RPC_OCC_MISMATCH: &str = "QUORUM_PRIVATE_RPC_OCC_MISMATCH";
const QUORUM_FAILURE_TOKEN_PRIVATE_RPC_INVALID_ARGUMENT: &str =
    "QUORUM_PRIVATE_RPC_INVALID_ARGUMENT";
const QUORUM_FAILURE_TOKEN_PRIVATE_RPC_ERROR: &str = "QUORUM_PRIVATE_RPC_ERROR";
const QUORUM_FAILURE_TOKEN_PRIVATE_RPC_TASK_JOIN: &str = "QUORUM_PRIVATE_RPC_TASK_JOIN";
const QUORUM_FAILURE_TOKEN_REPLICATION_FOLLOWER_ERROR: &str = "QUORUM_REPLICATION_FOLLOWER_ERROR";
const QUORUM_FAILURE_TOKEN_SIMULATION_DISABLED: &str = "QUORUM_SIMULATION_DISABLED";
pub(crate) const DEFAULT_WAL_GROUP_COMMIT_WINDOW_US: u64 = 200;
pub(crate) const DEFAULT_WAL_GROUP_COMMIT_MAX_OPS: usize = 2_048;
pub(crate) const DEFAULT_WAL_GROUP_COMMIT_MAX_BYTES: usize = 4 * 1024 * 1024;
pub(crate) const DEFAULT_WAL_SEGMENT_PREALLOCATE_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const DEFAULT_WAL_WRITEV_ENABLED: bool = true;

fn format_quorum_failure_reason(token: &str, detail: impl AsRef<str>) -> String {
    format!("{token}: {}", detail.as_ref())
}

fn retryable_quorum_limit_error(token: &str, detail: impl AsRef<str>) -> DbError {
    DbError::limit(format!(
        "{}; RETRY_AFTER_MS=25",
        format_quorum_failure_reason(token, detail)
    ))
}

fn replication_failure_token_for_message(message: &str) -> &'static str {
    let token = message.split(':').next().map(str::trim).unwrap_or_default();
    let token_like = !token.is_empty()
        && token
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch == '_' || ch.is_ascii_digit());
    if token_like {
        return match token {
            QUORUM_FAILURE_TOKEN_REPLICATION_RPC_BACKPRESSURE => {
                QUORUM_FAILURE_TOKEN_REPLICATION_RPC_BACKPRESSURE
            }
            QUORUM_FAILURE_TOKEN_PRIVATE_RPC_REQUIRED => QUORUM_FAILURE_TOKEN_PRIVATE_RPC_REQUIRED,
            QUORUM_FAILURE_TOKEN_PRIVATE_MESH_NOT_READY => {
                QUORUM_FAILURE_TOKEN_PRIVATE_MESH_NOT_READY
            }
            QUORUM_FAILURE_TOKEN_TARGET_SET_TOO_SMALL => QUORUM_FAILURE_TOKEN_TARGET_SET_TOO_SMALL,
            QUORUM_FAILURE_TOKEN_SIMULATION_DISABLED => QUORUM_FAILURE_TOKEN_SIMULATION_DISABLED,
            QUORUM_FAILURE_TOKEN_DURABILITY_NOT_REACHED => {
                QUORUM_FAILURE_TOKEN_DURABILITY_NOT_REACHED
            }
            QUORUM_FAILURE_TOKEN_PRIVATE_RPC_UNAVAILABLE => {
                QUORUM_FAILURE_TOKEN_PRIVATE_RPC_UNAVAILABLE
            }
            QUORUM_FAILURE_TOKEN_PRIVATE_RPC_RETRY_AFTER => {
                QUORUM_FAILURE_TOKEN_PRIVATE_RPC_RETRY_AFTER
            }
            QUORUM_FAILURE_TOKEN_PRIVATE_RPC_NOT_LEADER => {
                QUORUM_FAILURE_TOKEN_PRIVATE_RPC_NOT_LEADER
            }
            QUORUM_FAILURE_TOKEN_PRIVATE_RPC_OCC_MISMATCH => {
                QUORUM_FAILURE_TOKEN_PRIVATE_RPC_OCC_MISMATCH
            }
            QUORUM_FAILURE_TOKEN_PRIVATE_RPC_INVALID_ARGUMENT => {
                QUORUM_FAILURE_TOKEN_PRIVATE_RPC_INVALID_ARGUMENT
            }
            _ => QUORUM_FAILURE_TOKEN_REPLICATION_FOLLOWER_ERROR,
        };
    }
    if message.contains("private mesh replication task join failed") {
        return QUORUM_FAILURE_TOKEN_PRIVATE_RPC_TASK_JOIN;
    }
    if message.contains("private rpc failed") {
        return QUORUM_FAILURE_TOKEN_PRIVATE_RPC_ERROR;
    }
    QUORUM_FAILURE_TOKEN_REPLICATION_FOLLOWER_ERROR
}

fn majority_quorum_size(voter_count: usize) -> usize {
    (voter_count / 2) + 1
}

fn strict_replication_validation_message(
    replication_factor: u32,
    write_quorum: u32,
) -> Option<String> {
    if replication_factor == 0 {
        return Some("replication.factor must be > 0".to_string());
    }
    if write_quorum == 0 || write_quorum > replication_factor {
        return Some(format!(
            "replication.write_quorum must be within [1, {replication_factor}]"
        ));
    }
    let majority = (replication_factor / 2) + 1;
    if write_quorum < majority {
        return Some(format!(
            "replication.write_quorum must be majority quorum for replication factor {replication_factor} (min {majority})"
        ));
    }
    None
}

fn additional_durable_acks_needed(
    membership: &MembershipConfig,
    durable_acks: &BTreeSet<u64>,
) -> usize {
    let missing_for = |voters: &BTreeSet<u64>| {
        let acked = voters
            .iter()
            .filter(|node_id| durable_acks.contains(node_id))
            .count();
        majority_quorum_size(voters.len()).saturating_sub(acked)
    };
    if let Some(joint) = membership.joint() {
        let outgoing_missing = missing_for(&joint.outgoing_voters);
        let incoming_missing = missing_for(&joint.incoming_voters);
        outgoing_missing.max(incoming_missing)
    } else {
        missing_for(membership.voters())
    }
}

fn additional_acks_needed_for_quorum(
    membership: &MembershipConfig,
    durable_acks: &BTreeSet<u64>,
    write_quorum_required: usize,
) -> usize {
    let write_quorum_missing = write_quorum_required.saturating_sub(durable_acks.len());
    let durable_missing = additional_durable_acks_needed(membership, durable_acks);
    write_quorum_missing.max(durable_missing)
}

fn map_remote_voters_to_mesh_nodes(
    remote_voter_ids: &[u64],
    mut follower_nodes: Vec<String>,
) -> Vec<(u64, String)> {
    let mut sorted_voters = remote_voter_ids.to_vec();
    sorted_voters.sort_unstable();
    sorted_voters.dedup();
    follower_nodes.sort();
    follower_nodes.dedup();
    sorted_voters.into_iter().zip(follower_nodes).collect()
}

fn remote_voter_ids_sorted_by_rank(prepared: &PreparedOutsideLockBatch) -> Vec<u64> {
    let mut voter_ids = prepared.membership.voters().clone();
    if let Some(joint) = prepared.membership.joint() {
        voter_ids.extend(joint.outgoing_voters.iter().copied());
        voter_ids.extend(joint.incoming_voters.iter().copied());
    }
    let mut remote_voter_ids = voter_ids
        .into_iter()
        .filter(|node_id| *node_id != LOCAL_NODE_ID)
        .collect::<Vec<_>>();
    remote_voter_ids.sort_unstable();
    remote_voter_ids.dedup();
    remote_voter_ids.sort_unstable_by_key(|node_id| {
        (
            prepared
                .replica_latency_rank
                .get(node_id)
                .copied()
                .unwrap_or(u64::MAX),
            *node_id,
        )
    });
    remote_voter_ids
}

fn sorted_run_entries_from_records(
    records: &[Record],
) -> Vec<crate::db::lsm::sstable::SsTableEntry> {
    let mut entries = Vec::new();
    for record in records {
        let Ok(user_key) = encode_user_key(&record.namespace, &record.key) else {
            continue;
        };
        match record.kind {
            RecordKind::Put => entries.push(crate::db::lsm::sstable::SsTableEntry::live(
                user_key,
                record.version,
                record.value.to_vec(),
                None,
            )),
            RecordKind::Delete => entries.push(crate::db::lsm::sstable::SsTableEntry::tombstone(
                user_key,
                record.version,
            )),
            _ => {}
        }
    }
    entries.sort_unstable_by(|left, right| {
        left.key
            .cmp(&right.key)
            .then_with(|| left.version.cmp(&right.version))
    });
    entries
}

fn build_sorted_run_chunk_payloads_from_records(records: &[Record]) -> Vec<Vec<u8>> {
    let max_entries = sorted_run_catchup_chunk_max_entries_default().max(1);
    let max_bytes = sorted_run_catchup_chunk_max_bytes_default().max(1024);
    let entries = sorted_run_entries_from_records(records);
    if entries.is_empty() {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    let mut current_entries = Vec::new();
    let mut current_estimated_bytes = 0usize;
    for entry in entries {
        let entry_estimated_bytes = crate::db::lsm::sstable::estimated_entry_bytes(&entry).max(1);
        let chunk_full = !current_entries.is_empty()
            && (current_entries.len() >= max_entries
                || current_estimated_bytes.saturating_add(entry_estimated_bytes) > max_bytes);
        if chunk_full {
            chunks.push(crate::db::lsm::sstable::encode_block(&current_entries));
            current_entries.clear();
            current_estimated_bytes = 0;
        }
        current_estimated_bytes = current_estimated_bytes.saturating_add(entry_estimated_bytes);
        current_entries.push(entry);
    }
    if !current_entries.is_empty() {
        chunks.push(crate::db::lsm::sstable::encode_block(&current_entries));
    }
    chunks
}

async fn send_sorted_run_chunks_to_target_over_private_mesh(
    target_addr: &str,
    term: u64,
    chunk_stream_id: u64,
    chunk_payloads: &[Vec<u8>],
    timeout: Duration,
) -> Result<u64, crate::db::rpc::errors::RpcError> {
    let total_chunks = chunk_payloads.len() as u64;
    if total_chunks == 0 {
        return Ok(0);
    }
    let mut next_chunk_index = 0u64;
    let mut sent_chunks = 0u64;
    while next_chunk_index < total_chunks {
        let payload = chunk_payloads
            .get(next_chunk_index as usize)
            .cloned()
            .ok_or_else(|| crate::db::rpc::errors::RpcError {
                code: crate::db::rpc::errors::RpcStatusCode::InvalidArgument,
                message: format!(
                    "SORTED_RUN_CATCHUP_LOCAL_CHUNK_MISSING: next_chunk_index={next_chunk_index} total_chunks={total_chunks}"
                ),
                retry: None,
                leader: None,
            })?;
        let req = crate::db::rpc::tonic_service::wrpc::SortedRunCatchUpChunkRequest {
            handle: 0,
            term,
            chunk_stream_id,
            chunk_index: next_chunk_index,
            total_chunks,
            payload: bytes::Bytes::from(payload),
        };
        let resp = crate::db::rpc::private_network::replica_install_sorted_run_chunk_over_private_rpc_async(
            target_addr,
            req,
            timeout,
        )
        .await?;
        if !resp.accepted {
            return Err(crate::db::rpc::errors::RpcError {
                code: crate::db::rpc::errors::RpcStatusCode::RetryAfter,
                message: format!(
                    "SORTED_RUN_CATCHUP_REJECTED: reason={} next_chunk_index={} total_chunks={}",
                    resp.rejection_reason, resp.next_chunk_index, total_chunks
                ),
                retry: Some(crate::db::rpc::errors::RetryHint { retry_after_ms: 25 }),
                leader: None,
            });
        }
        let advanced = resp
            .next_chunk_index
            .saturating_sub(next_chunk_index)
            .max(1);
        sent_chunks = sent_chunks.saturating_add(advanced);
        next_chunk_index = resp.next_chunk_index.min(total_chunks);
    }
    Ok(sent_chunks)
}

fn replicate_prepared_batch_over_private_mesh(
    mesh: &PrivateMeshContext,
    prepared: &PreparedOutsideLockBatch,
) -> Result<OutsideLockFanoutResult, OutsideLockReplicationError> {
    let replicate_started = Instant::now();
    let active_group_id = prepared.active_group_id;
    let required_term = prepared.required_term;
    let required_index = prepared.required_index;
    let remote_voter_ids = remote_voter_ids_sorted_by_rank(prepared);
    let remote_voter_ids_for_mapping = remote_voter_ids.clone();
    let total_target_count = remote_voter_ids.len();
    let mut contacted_target_count = 0usize;
    let mut replication_wave_count = 0usize;
    let mut replication_wave_total_targets = 0usize;
    let mut replication_wave_max_targets = 0usize;
    let mut successful_target_count = 0usize;
    let mut failed_target_count = 0usize;
    let mut cancelled_target_count = 0usize;
    let mut aborted_in_flight_count = 0usize;
    let replication_max_in_flight = REPLICATION_MAX_IN_FLIGHT;
    let replication_max_targets = REPLICATION_MAX_TARGETS;
    if remote_voter_ids.len() > replication_max_targets {
        let detail = format!(
            "replication target count {} exceeds max targets {}",
            remote_voter_ids.len(),
            replication_max_targets
        );
        return Err(OutsideLockReplicationError {
            token: QUORUM_FAILURE_TOKEN_REPLICATION_IN_FLIGHT_LIMIT,
            detail,
            total_target_count,
            contacted_target_count,
            replication_wave_count,
            replication_wave_total_targets,
            replication_wave_max_targets,
            successful_target_count,
            failed_target_count,
            cancelled_target_count,
            aborted_in_flight_count,
            replication_error: None,
        });
    }
    if let Err(err) = mesh.ensure_ready_for("replication quorum write") {
        let detail = format!("private mesh readiness check failed: {}", err.message);
        return Err(OutsideLockReplicationError {
            token: QUORUM_FAILURE_TOKEN_PRIVATE_MESH_NOT_READY,
            detail,
            total_target_count,
            contacted_target_count,
            replication_wave_count,
            replication_wave_total_targets,
            replication_wave_max_targets,
            successful_target_count,
            failed_target_count,
            cancelled_target_count,
            aborted_in_flight_count,
            replication_error: None,
        });
    }
    let node_mapping =
        map_remote_voters_to_mesh_nodes(&remote_voter_ids_for_mapping, mesh.follower_nodes());
    if node_mapping.len() < remote_voter_ids.len() {
        let detail = format!(
            "replication quorum target set too small: followers={} voters_required={}",
            node_mapping.len(),
            remote_voter_ids.len()
        );
        return Err(OutsideLockReplicationError {
            token: QUORUM_FAILURE_TOKEN_TARGET_SET_TOO_SMALL,
            detail,
            total_target_count,
            contacted_target_count,
            replication_wave_count,
            replication_wave_total_targets,
            replication_wave_max_targets,
            successful_target_count,
            failed_target_count,
            cancelled_target_count,
            aborted_in_flight_count,
            replication_error: None,
        });
    }

    let mut replication_error: Option<DbError> = None;
    let mut replication_targets = Vec::with_capacity(remote_voter_ids.len());
    for (node_id, node_name) in node_mapping {
        let Some(address) = mesh.address_for_node(&node_name) else {
            if replication_error.is_none() {
                replication_error = Some(DbError::limit(format!(
                    "private mesh follower address missing for {node_name}; RETRY_AFTER_MS=25"
                )));
            }
            continue;
        };
        replication_targets.push((node_id, address));
    }
    replication_targets.sort_unstable_by_key(|(node_id, _)| {
        (
            prepared
                .replica_latency_rank
                .get(node_id)
                .copied()
                .unwrap_or(u64::MAX),
            *node_id,
        )
    });

    let request_template = crate::db::rpc::grpc::WriteBatchRequest {
        handle: 0,
        ops: prepared.batch_ops.clone(),
        idempotency_token: None,
        expected_home_epoch: prepared.ownership_fence.expected_home_epoch,
        expected_shard_map_epoch: prepared.ownership_fence.expected_shard_map_epoch,
        ownership_token: prepared.ownership_fence.ownership_token.clone(),
    };
    let mut proto_request_template =
        crate::db::rpc::tonic_service::write_batch_request_to_proto(request_template);
    // Attach pre-encoded WAL bytes so followers can write them directly to WAL,
    // bypassing the writer-lane queue and WAL re-encoding (Opt 2).
    let wal_payload =
        encode_records_to_wal_bytes(&prepared.staged_records, |bytes, _| bytes.to_vec());
    proto_request_template.wal_payload = Some(Bytes::from(wal_payload));
    let io_timeout = mesh.io_timeout;
    let sorted_run_lag_threshold_ops = sorted_run_catchup_lag_threshold_ops_default();
    let sorted_run_chunk_payloads = Arc::new(build_sorted_run_chunk_payloads_from_records(
        &prepared.staged_records,
    ));
    let total_targets = replication_targets.len();
    let mut hedge_extra = REPLICATION_HEDGE_EXTRA;
    if hedge_extra > 0 && REPLICATION_DYNAMIC_HEDGE {
        let rpc_snapshot = crate::db::rpc::private_network::replication_rpc_in_flight_snapshot();
        if rpc_snapshot.available_permits.saturating_mul(2) < rpc_snapshot.max_in_flight {
            hedge_extra = 0;
        }
    }
    let mut follower_responses = Vec::new();
    let quorum_satisfied = |acks: &BTreeSet<u64>| {
        prepared.membership.has_durable_quorum(acks) && acks.len() >= prepared.write_quorum_required
    };
    let mut provisional_durable_acks = BTreeSet::from([LOCAL_NODE_ID]);
    let mut follower_progress_updates = Vec::new();
    let mut sorted_run_chunks_sent = 0u64;
    let mut wave_start = 0usize;
    while wave_start < total_targets {
        let remaining_targets = total_targets.saturating_sub(wave_start);
        let additional_needed = additional_acks_needed_for_quorum(
            &prepared.membership,
            &provisional_durable_acks,
            prepared.write_quorum_required,
        )
        .max(1);
        let wave_size = additional_needed
            .saturating_add(hedge_extra)
            .min(replication_max_in_flight.max(1))
            .min(remaining_targets.max(1));
        replication_wave_count = replication_wave_count.saturating_add(1);
        replication_wave_total_targets = replication_wave_total_targets.saturating_add(wave_size);
        replication_wave_max_targets = replication_wave_max_targets.max(wave_size);
        let wave_end = (wave_start + wave_size).min(total_targets);
        let (fanout_results, wave_aborted_count) = if wave_size == 1 {
            let (node_id, address) = replication_targets[wave_start].clone();
            let follower_progress = prepared
                .follower_progress_hints
                .get(&node_id)
                .copied()
                .unwrap_or(0);
            let transfer_mode = select_transfer_mode(
                follower_progress,
                required_index,
                sorted_run_lag_threshold_ops,
            );
            let should_send_sorted_run = !sorted_run_chunk_payloads.is_empty()
                && matches!(transfer_mode, CatchUpTransferMode::SortedRunThenTail);
            let sorted_run_stream_id = payload_hash64(
                format!(
                    "sorted-run-{}-{}-{}-{node_id}",
                    active_group_id, required_term, required_index
                )
                .as_bytes(),
            );
            let chunk_payloads = sorted_run_chunk_payloads.clone();
            let token = format!(
                "mesh-quorum-{}-{}-{}-{node_id}",
                active_group_id, required_term, required_index
            );
            let mut proto_request = proto_request_template.clone();
            proto_request.idempotency_token = Some(token);
            let result = block_on_runtime(async move {
                let fanout_started = Instant::now();
                let mut sent_sorted_chunks = 0u64;
                let response = if should_send_sorted_run {
                    match send_sorted_run_chunks_to_target_over_private_mesh(
                        &address,
                        required_term,
                        sorted_run_stream_id,
                        chunk_payloads.as_slice(),
                        io_timeout,
                    )
                    .await
                    {
                        Ok(sent) => {
                            sent_sorted_chunks = sent;
                            crate::db::rpc::private_network::replicate_write_batch_proto_prefer_stream_async(
                                &address,
                                proto_request,
                                io_timeout,
                            )
                            .await
                        }
                        Err(err) => Err(err),
                    }
                } else {
                    crate::db::rpc::private_network::replicate_write_batch_proto_prefer_stream_async(
                        &address,
                        proto_request,
                        io_timeout,
                    )
                    .await
                };
                (
                    node_id,
                    duration_to_nanos(fanout_started.elapsed()).max(1),
                    sent_sorted_chunks,
                    response,
                )
            });
            (vec![Ok(result)], 0usize)
        } else {
            block_on_runtime(async {
                let mut join_set = tokio::task::JoinSet::new();
                for idx in wave_start..wave_end {
                    let (node_id, address) = replication_targets[idx].clone();
                    let follower_progress = prepared
                        .follower_progress_hints
                        .get(&node_id)
                        .copied()
                        .unwrap_or(0);
                    let transfer_mode = select_transfer_mode(
                        follower_progress,
                        required_index,
                        sorted_run_lag_threshold_ops,
                    );
                    let should_send_sorted_run = !sorted_run_chunk_payloads.is_empty()
                        && matches!(transfer_mode, CatchUpTransferMode::SortedRunThenTail);
                    let sorted_run_stream_id = payload_hash64(
                        format!(
                            "sorted-run-{}-{}-{}-{node_id}",
                            active_group_id, required_term, required_index
                        )
                        .as_bytes(),
                    );
                    let chunk_payloads = sorted_run_chunk_payloads.clone();
                    let token = format!(
                        "mesh-quorum-{}-{}-{}-{node_id}",
                        active_group_id, required_term, required_index
                    );
                    let mut proto_request = proto_request_template.clone();
                    proto_request.idempotency_token = Some(token);
                    join_set.spawn(async move {
                        let fanout_started = Instant::now();
                        let mut sent_sorted_chunks = 0u64;
                        let response = if should_send_sorted_run {
                            match send_sorted_run_chunks_to_target_over_private_mesh(
                                &address,
                                required_term,
                                sorted_run_stream_id,
                                chunk_payloads.as_slice(),
                                io_timeout,
                            )
                            .await
                            {
                                Ok(sent) => {
                                    sent_sorted_chunks = sent;
                                    crate::db::rpc::private_network::replicate_write_batch_proto_prefer_stream_async(
                                        &address,
                                        proto_request,
                                        io_timeout,
                                    )
                                    .await
                                }
                                Err(err) => Err(err),
                            }
                        } else {
                            crate::db::rpc::private_network::replicate_write_batch_proto_prefer_stream_async(
                                &address,
                                proto_request,
                                io_timeout,
                            )
                            .await
                        };
                        (
                            node_id,
                            duration_to_nanos(fanout_started.elapsed()).max(1),
                            sent_sorted_chunks,
                            response,
                        )
                    });
                }

                let mut joined = Vec::new();
                let mut successful_acks = 0usize;
                let mut aborted_count = 0usize;
                while let Some(result) = join_set.join_next().await {
                    let mut reached_wave_quorum = false;
                    if let Ok((_node_id, _latency_ns, _sorted_chunks_sent, Ok(_))) = &result {
                        successful_acks = successful_acks.saturating_add(1);
                        if successful_acks >= additional_needed {
                            reached_wave_quorum = true;
                        }
                    }
                    joined.push(result);
                    if reached_wave_quorum {
                        if !join_set.is_empty() {
                            aborted_count = aborted_count.saturating_add(join_set.len());
                            join_set.abort_all();
                        }
                        break;
                    }
                }
                (joined, aborted_count)
            })
        };
        aborted_in_flight_count = aborted_in_flight_count.saturating_add(wave_aborted_count);

        for result in fanout_results {
            match result {
                Ok((node_id, latency_ns, sorted_chunks_for_node, Ok(resp))) => {
                    contacted_target_count = contacted_target_count.saturating_add(1);
                    successful_target_count = successful_target_count.saturating_add(1);
                    provisional_durable_acks.insert(node_id);
                    sorted_run_chunks_sent =
                        sorted_run_chunks_sent.saturating_add(sorted_chunks_for_node);
                    follower_progress_updates.push((node_id, prepared.required_index));
                    // Use follower-reported WAL fsync time when available;
                    // fall back to full RPC latency for old followers.
                    let fsync_ns = resp.follower_wal_fsync_ns.unwrap_or(latency_ns);
                    follower_responses.push(FollowerAppendResponse {
                        node_id,
                        response: AppendEntriesResponse {
                            term: prepared.required_term,
                            success: true,
                            match_index: prepared.required_index,
                            conflict_index: None,
                        },
                        replication_latency_ns: latency_ns,
                        fsync_latency_ns: fsync_ns,
                    });
                }
                Ok((_node_id, _latency_ns, _sorted_chunks_for_node, Err(err))) => {
                    contacted_target_count = contacted_target_count.saturating_add(1);
                    failed_target_count = failed_target_count.saturating_add(1);
                    let mapped = map_private_rpc_error(err);
                    if replication_error.is_none() {
                        replication_error = Some(mapped);
                    }
                }
                Err(err) => {
                    if err.is_cancelled() {
                        cancelled_target_count = cancelled_target_count.saturating_add(1);
                        continue;
                    }
                    failed_target_count = failed_target_count.saturating_add(1);
                    if replication_error.is_none() {
                        replication_error = Some(DbError::limit(format!(
                            "private mesh replication task join failed: {err}; RETRY_AFTER_MS=25"
                        )));
                    }
                }
            }
        }
        if quorum_satisfied(&provisional_durable_acks) {
            break;
        }
        wave_start = wave_end;
    }

    Ok(OutsideLockFanoutResult {
        replicate_ns: duration_to_nanos(replicate_started.elapsed()),
        used_simulation: false,
        sorted_run_chunks_sent,
        total_target_count,
        contacted_target_count,
        replication_wave_count,
        replication_wave_total_targets,
        replication_wave_max_targets,
        successful_target_count,
        failed_target_count,
        cancelled_target_count,
        aborted_in_flight_count,
        follower_responses,
        follower_state_updates: Vec::new(),
        follower_progress_updates,
        replication_error,
    })
}

fn replicate_prepared_batch_with_local_simulation(
    prepared: &PreparedOutsideLockBatch,
) -> Result<OutsideLockFanoutResult, OutsideLockReplicationError> {
    let replicate_started = Instant::now();
    let remote_voter_ids = remote_voter_ids_sorted_by_rank(prepared);
    let total_target_count = remote_voter_ids.len();
    let replication_max_targets = REPLICATION_MAX_TARGETS;
    if remote_voter_ids.len() > replication_max_targets {
        let detail = format!(
            "replication target count {} exceeds max targets {}",
            remote_voter_ids.len(),
            replication_max_targets
        );
        return Err(OutsideLockReplicationError {
            token: QUORUM_FAILURE_TOKEN_REPLICATION_IN_FLIGHT_LIMIT,
            detail,
            total_target_count,
            contacted_target_count: 0,
            replication_wave_count: 0,
            replication_wave_total_targets: 0,
            replication_wave_max_targets: 0,
            successful_target_count: 0,
            failed_target_count: 0,
            cancelled_target_count: 0,
            aborted_in_flight_count: 0,
            replication_error: None,
        });
    }
    if prepared.require_private_rpc_transport && !remote_voter_ids.is_empty() {
        let detail = format!(
            "quorum transport requires private rpc but mesh leader path unavailable; voters_required={}",
            remote_voter_ids.len()
        );
        return Err(OutsideLockReplicationError {
            token: QUORUM_FAILURE_TOKEN_PRIVATE_RPC_REQUIRED,
            detail,
            total_target_count,
            contacted_target_count: 0,
            replication_wave_count: 0,
            replication_wave_total_targets: 0,
            replication_wave_max_targets: 0,
            successful_target_count: 0,
            failed_target_count: 0,
            cancelled_target_count: 0,
            aborted_in_flight_count: 0,
            replication_error: None,
        });
    }
    if !remote_voter_ids.is_empty() && !prepared.simulation_fallback_allowed {
        let detail = format!(
            "replication fallback to local simulation disabled; voters_required={}",
            remote_voter_ids.len()
        );
        return Err(OutsideLockReplicationError {
            token: QUORUM_FAILURE_TOKEN_SIMULATION_DISABLED,
            detail,
            total_target_count,
            contacted_target_count: 0,
            replication_wave_count: 0,
            replication_wave_total_targets: 0,
            replication_wave_max_targets: 0,
            successful_target_count: 0,
            failed_target_count: 0,
            cancelled_target_count: 0,
            aborted_in_flight_count: 0,
            replication_error: None,
        });
    }

    let mut follower_states: HashMap<u64, NodeState> =
        prepared.follower_snapshots.iter().cloned().collect();
    let mut contacted_target_count = 0usize;
    let mut replication_wave_count = 0usize;
    let mut replication_wave_total_targets = 0usize;
    let mut replication_wave_max_targets = 0usize;
    let mut successful_target_count = 0usize;
    let mut failed_target_count = 0usize;
    let cancelled_target_count = 0usize;
    let aborted_in_flight_count = 0usize;
    let quorum_satisfied = |acks: &BTreeSet<u64>| {
        prepared.membership.has_durable_quorum(acks) && acks.len() >= prepared.write_quorum_required
    };
    let mut provisional_durable_acks = BTreeSet::from([LOCAL_NODE_ID]);
    let mut follower_responses = Vec::new();
    let mut follower_progress_updates = Vec::new();
    let mut replication_error: Option<DbError> = None;
    for node_id in remote_voter_ids {
        replication_wave_count = replication_wave_count.saturating_add(1);
        replication_wave_total_targets = replication_wave_total_targets.saturating_add(1);
        replication_wave_max_targets = replication_wave_max_targets.max(1);
        contacted_target_count = contacted_target_count.saturating_add(1);
        let follower_state = follower_states
            .entry(node_id)
            .or_insert_with(|| NodeState::with_timing(node_id, 0, 10));
        let fanout_started = Instant::now();
        match replicate_to_follower(
            &prepared.leader_snapshot,
            follower_state,
            prepared.leader_commit,
        ) {
            Ok(response) => {
                let replication_latency_ns = duration_to_nanos(fanout_started.elapsed()).max(1);
                if response_is_durable_ack(
                    &response,
                    prepared.required_term,
                    prepared.required_index,
                ) {
                    provisional_durable_acks.insert(node_id);
                }
                if response.success {
                    successful_target_count = successful_target_count.saturating_add(1);
                } else {
                    failed_target_count = failed_target_count.saturating_add(1);
                }
                follower_responses.push(FollowerAppendResponse {
                    node_id,
                    response,
                    replication_latency_ns,
                    fsync_latency_ns: replication_latency_ns,
                });
                follower_progress_updates.push((node_id, prepared.required_index));
            }
            Err(err) => {
                failed_target_count = failed_target_count.saturating_add(1);
                if replication_error.is_none() {
                    replication_error = Some(err);
                }
            }
        }
        if quorum_satisfied(&provisional_durable_acks) {
            break;
        }
    }

    Ok(OutsideLockFanoutResult {
        replicate_ns: duration_to_nanos(replicate_started.elapsed()),
        used_simulation: true,
        sorted_run_chunks_sent: 0,
        total_target_count,
        contacted_target_count,
        replication_wave_count,
        replication_wave_total_targets,
        replication_wave_max_targets,
        successful_target_count,
        failed_target_count,
        cancelled_target_count,
        aborted_in_flight_count,
        follower_responses,
        follower_state_updates: follower_states.into_iter().collect(),
        follower_progress_updates,
        replication_error,
    })
}

#[derive(Debug, Clone, Copy)]
struct WriteFlushTuning {
    window: Duration,
    max_ops: usize,
    soft_bytes: usize,
}

impl Default for WriteFlushTuning {
    fn default() -> Self {
        Self {
            window: WRITE_FLUSH_WINDOW,
            max_ops: WRITE_FLUSH_MAX_OPS,
            soft_bytes: WRITE_FLUSH_SOFT_BYTES,
        }
    }
}

impl WriteFlushTuning {
    fn dynamic_for_queue(self, queue_depth: usize, shard_local_depth: usize) -> Self {
        let mut tuned = self;
        if queue_depth >= Self::saturation_threshold() || shard_local_depth >= self.max_ops {
            tuned.window = Duration::from_millis(0);
            tuned.max_ops = tuned.max_ops.saturating_mul(4).min(MAX_BATCH_OPS);
            tuned.soft_bytes = tuned.soft_bytes.saturating_mul(4).min(MAX_BATCH_BYTES);
            return tuned;
        }
        // Activate scaling sooner: capacity/4 and max_ops/4 instead of /2.
        if queue_depth >= (Self::capacity() / 4).max(1)
            || shard_local_depth >= (self.max_ops / 4).max(1)
        {
            tuned.window = self.window.min(Duration::from_micros(500));
            tuned.max_ops = tuned.max_ops.saturating_mul(2).min(MAX_BATCH_OPS);
            tuned.soft_bytes = tuned.soft_bytes.saturating_mul(2).min(MAX_BATCH_BYTES);
        }
        tuned
    }

    fn capacity() -> usize {
        MAX_BATCH_OPS.saturating_mul(WRITE_LANE_QUEUE_MULTIPLIER)
    }

    fn saturation_threshold() -> usize {
        (Self::capacity().saturating_mul(WRITE_LANE_SATURATION_PCT)).max(1) / 100
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DbWriteStageSample {
    pub op_count: u64,
    pub byte_count: u64,
    pub queue_wait_ns: u64,
    pub engine_lock_wait_ns: u64,
    pub validate_route_ns: u64,
    pub replicate_ns: u64,
    pub wal_append_ns: u64,
    pub wal_submit_wait_ns: u64,
    pub wal_hol_wait_ns: u64,
    pub wal_queue_wait_ns: u64,
    pub wal_encode_ns: u64,
    pub wal_fdatasync_ns: u64,
    pub wal_mutex_wait_ns: u64,
    pub apply_ns: u64,
    pub raft_persist_ns: u64,
    pub clock_persist_ns: u64,
    pub total_ns: u64,
    pub lane_dequeue_to_complete_ns: u64,
    pub queue_to_complete_ns: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DbWriteStageAggregate {
    pub sample_count: u64,
    pub op_count: u64,
    pub byte_count: u64,
    pub avg_queue_wait_us: f64,
    pub avg_lane_dequeue_to_complete_us: f64,
    pub avg_queue_to_complete_us: f64,
    pub queue_saturation_pct: f64,
    pub retry_after_pct: f64,
    pub engine_lock_wait_pct: f64,
    pub validate_route_pct: f64,
    pub replicate_pct: f64,
    pub wal_append_pct: f64,
    pub wal_submit_wait_pct: f64,
    pub wal_hol_wait_pct: f64,
    pub wal_queue_wait_pct: f64,
    pub wal_encode_pct: f64,
    pub wal_fdatasync_pct: f64,
    pub wal_mutex_wait_pct: f64,
    pub apply_pct: f64,
    pub raft_persist_pct: f64,
    pub clock_persist_pct: f64,
    pub other_pct: f64,
    pub recent_samples: Vec<DbWriteStageSample>,
}

impl Default for DbWriteStageAggregate {
    fn default() -> Self {
        Self {
            sample_count: 0,
            op_count: 0,
            byte_count: 0,
            avg_queue_wait_us: 0.0,
            avg_lane_dequeue_to_complete_us: 0.0,
            avg_queue_to_complete_us: 0.0,
            queue_saturation_pct: 0.0,
            retry_after_pct: 0.0,
            engine_lock_wait_pct: 0.0,
            validate_route_pct: 0.0,
            replicate_pct: 0.0,
            wal_append_pct: 0.0,
            wal_submit_wait_pct: 0.0,
            wal_hol_wait_pct: 0.0,
            wal_queue_wait_pct: 0.0,
            wal_encode_pct: 0.0,
            wal_fdatasync_pct: 0.0,
            wal_mutex_wait_pct: 0.0,
            apply_pct: 0.0,
            raft_persist_pct: 0.0,
            clock_persist_pct: 0.0,
            other_pct: 0.0,
            recent_samples: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DbClientWritePathSample {
    pub preflight_ns: u64,
    pub enqueue_wait_ns: u64,
    pub response_wait_ns: u64,
    pub remote_forward_ns: u64,
    pub total_ns: u64,
    pub forwarded: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DbClientWritePathAggregate {
    pub sample_count: u64,
    pub forwarded_count: u64,
    pub avg_total_us: f64,
    pub preflight_pct: f64,
    pub enqueue_wait_pct: f64,
    pub response_wait_pct: f64,
    pub remote_forward_pct: f64,
    pub other_pct: f64,
    pub recent_samples: Vec<DbClientWritePathSample>,
}

impl Default for DbClientWritePathAggregate {
    fn default() -> Self {
        Self {
            sample_count: 0,
            forwarded_count: 0,
            avg_total_us: 0.0,
            preflight_pct: 0.0,
            enqueue_wait_pct: 0.0,
            response_wait_pct: 0.0,
            remote_forward_pct: 0.0,
            other_pct: 0.0,
            recent_samples: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DbWalFlushStats {
    pub flushes: u64,
    pub avg_ops_per_flush: f64,
    pub avg_bytes_per_flush: f64,
    pub fsync_failures: u64,
    pub forced_flushes_on_close: u64,
}

impl From<WalFlushStats> for DbWalFlushStats {
    fn from(value: WalFlushStats) -> Self {
        Self {
            flushes: value.flushes,
            avg_ops_per_flush: value.avg_ops_per_flush,
            avg_bytes_per_flush: value.avg_bytes_per_flush,
            fsync_failures: value.fsync_failures,
            forced_flushes_on_close: value.forced_flushes_on_close,
        }
    }
}

#[derive(Debug, Default)]
struct WriteStageTelemetry {
    sample_count: u64,
    op_count: u64,
    byte_count: u64,
    queue_wait_ns: u128,
    lane_dequeue_to_complete_ns: u128,
    queue_to_complete_ns: u128,
    engine_lock_wait_ns: u128,
    validate_route_ns: u128,
    replicate_ns: u128,
    wal_append_ns: u128,
    wal_submit_wait_ns: u128,
    wal_hol_wait_ns: u128,
    wal_queue_wait_ns: u128,
    wal_encode_ns: u128,
    wal_fdatasync_ns: u128,
    wal_mutex_wait_ns: u128,
    apply_ns: u128,
    raft_persist_ns: u128,
    clock_persist_ns: u128,
    total_ns: u128,
    recent_samples: VecDeque<DbWriteStageSample>,
}

#[derive(Debug, Default)]
struct ClientWritePathTelemetry {
    sample_count: u64,
    forwarded_count: u64,
    preflight_ns: u128,
    enqueue_wait_ns: u128,
    response_wait_ns: u128,
    remote_forward_ns: u128,
    total_ns: u128,
    recent_samples: VecDeque<DbClientWritePathSample>,
}

impl WriteStageTelemetry {
    const RECENT_CAP: usize = 65_536;

    fn record(&mut self, sample: DbWriteStageSample) {
        self.sample_count = self.sample_count.saturating_add(1);
        self.op_count = self.op_count.saturating_add(sample.op_count);
        self.byte_count = self.byte_count.saturating_add(sample.byte_count);
        self.queue_wait_ns = self
            .queue_wait_ns
            .saturating_add(sample.queue_wait_ns as u128);
        self.lane_dequeue_to_complete_ns = self
            .lane_dequeue_to_complete_ns
            .saturating_add(sample.lane_dequeue_to_complete_ns as u128);
        self.queue_to_complete_ns = self
            .queue_to_complete_ns
            .saturating_add(sample.queue_to_complete_ns as u128);
        self.engine_lock_wait_ns = self
            .engine_lock_wait_ns
            .saturating_add(sample.engine_lock_wait_ns as u128);
        self.validate_route_ns = self
            .validate_route_ns
            .saturating_add(sample.validate_route_ns as u128);
        self.replicate_ns = self
            .replicate_ns
            .saturating_add(sample.replicate_ns as u128);
        self.wal_append_ns = self
            .wal_append_ns
            .saturating_add(sample.wal_append_ns as u128);
        self.wal_submit_wait_ns = self
            .wal_submit_wait_ns
            .saturating_add(sample.wal_submit_wait_ns as u128);
        self.wal_hol_wait_ns = self
            .wal_hol_wait_ns
            .saturating_add(sample.wal_hol_wait_ns as u128);
        self.wal_queue_wait_ns = self
            .wal_queue_wait_ns
            .saturating_add(sample.wal_queue_wait_ns as u128);
        self.wal_encode_ns = self
            .wal_encode_ns
            .saturating_add(sample.wal_encode_ns as u128);
        self.wal_fdatasync_ns = self
            .wal_fdatasync_ns
            .saturating_add(sample.wal_fdatasync_ns as u128);
        self.wal_mutex_wait_ns = self
            .wal_mutex_wait_ns
            .saturating_add(sample.wal_mutex_wait_ns as u128);
        self.apply_ns = self.apply_ns.saturating_add(sample.apply_ns as u128);
        self.raft_persist_ns = self
            .raft_persist_ns
            .saturating_add(sample.raft_persist_ns as u128);
        self.clock_persist_ns = self
            .clock_persist_ns
            .saturating_add(sample.clock_persist_ns as u128);
        self.total_ns = self.total_ns.saturating_add(sample.total_ns as u128);
        if self.recent_samples.len() >= Self::RECENT_CAP {
            self.recent_samples.pop_front();
        }
        self.recent_samples.push_back(sample);
    }

    fn snapshot(&self, queue: WriteLaneTelemetrySnapshot) -> DbWriteStageAggregate {
        let sample_count = self.sample_count.max(1);
        let stage_total = self
            .engine_lock_wait_ns
            .saturating_add(self.validate_route_ns)
            .saturating_add(self.replicate_ns)
            .saturating_add(self.wal_append_ns)
            .saturating_add(self.apply_ns)
            .saturating_add(self.raft_persist_ns)
            .saturating_add(self.clock_persist_ns);
        let denom = self.total_ns.max(1);
        let to_pct = |value: u128| ((value as f64) * 100.0) / (denom as f64);
        let queue_saturation_pct = if queue.depth_samples == 0 {
            0.0
        } else {
            (queue.saturated_samples as f64) * 100.0 / (queue.depth_samples as f64)
        };
        let retry_after_pct = if queue.enqueue_attempts == 0 {
            0.0
        } else {
            (queue.enqueue_rejections as f64) * 100.0 / (queue.enqueue_attempts as f64)
        };
        DbWriteStageAggregate {
            sample_count: self.sample_count,
            op_count: self.op_count,
            byte_count: self.byte_count,
            avg_queue_wait_us: (self.queue_wait_ns as f64) / (sample_count as f64) / 1_000.0,
            avg_lane_dequeue_to_complete_us: (self.lane_dequeue_to_complete_ns as f64)
                / (sample_count as f64)
                / 1_000.0,
            avg_queue_to_complete_us: (self.queue_to_complete_ns as f64)
                / (sample_count as f64)
                / 1_000.0,
            queue_saturation_pct,
            retry_after_pct,
            engine_lock_wait_pct: to_pct(self.engine_lock_wait_ns),
            validate_route_pct: to_pct(self.validate_route_ns),
            replicate_pct: to_pct(self.replicate_ns),
            wal_append_pct: to_pct(self.wal_append_ns),
            wal_submit_wait_pct: to_pct(self.wal_submit_wait_ns),
            wal_hol_wait_pct: to_pct(self.wal_hol_wait_ns),
            wal_queue_wait_pct: to_pct(self.wal_queue_wait_ns),
            wal_encode_pct: to_pct(self.wal_encode_ns),
            wal_fdatasync_pct: to_pct(self.wal_fdatasync_ns),
            wal_mutex_wait_pct: to_pct(self.wal_mutex_wait_ns),
            apply_pct: to_pct(self.apply_ns),
            raft_persist_pct: to_pct(self.raft_persist_ns),
            clock_persist_pct: to_pct(self.clock_persist_ns),
            other_pct: to_pct(denom.saturating_sub(stage_total)),
            recent_samples: self.recent_samples.iter().copied().collect(),
        }
    }
}

impl ClientWritePathTelemetry {
    const RECENT_CAP: usize = 65_536;

    fn record(&mut self, sample: DbClientWritePathSample) {
        self.sample_count = self.sample_count.saturating_add(1);
        if sample.forwarded {
            self.forwarded_count = self.forwarded_count.saturating_add(1);
        }
        self.preflight_ns = self
            .preflight_ns
            .saturating_add(sample.preflight_ns as u128);
        self.enqueue_wait_ns = self
            .enqueue_wait_ns
            .saturating_add(sample.enqueue_wait_ns as u128);
        self.response_wait_ns = self
            .response_wait_ns
            .saturating_add(sample.response_wait_ns as u128);
        self.remote_forward_ns = self
            .remote_forward_ns
            .saturating_add(sample.remote_forward_ns as u128);
        self.total_ns = self.total_ns.saturating_add(sample.total_ns as u128);
        if self.recent_samples.len() >= Self::RECENT_CAP {
            self.recent_samples.pop_front();
        }
        self.recent_samples.push_back(sample);
    }

    fn snapshot(&self) -> DbClientWritePathAggregate {
        if self.sample_count == 0 {
            return DbClientWritePathAggregate::default();
        }
        let denom = self.total_ns.max(1);
        let to_pct = |value: u128| ((value as f64) * 100.0) / (denom as f64);
        let stage_total = self
            .preflight_ns
            .saturating_add(self.enqueue_wait_ns)
            .saturating_add(self.response_wait_ns)
            .saturating_add(self.remote_forward_ns);
        DbClientWritePathAggregate {
            sample_count: self.sample_count,
            forwarded_count: self.forwarded_count,
            avg_total_us: (self.total_ns as f64) / (self.sample_count as f64) / 1_000.0,
            preflight_pct: to_pct(self.preflight_ns),
            enqueue_wait_pct: to_pct(self.enqueue_wait_ns),
            response_wait_pct: to_pct(self.response_wait_ns),
            remote_forward_pct: to_pct(self.remote_forward_ns),
            other_pct: to_pct(denom.saturating_sub(stage_total)),
            recent_samples: self.recent_samples.iter().copied().collect(),
        }
    }
}

#[derive(Debug, Default)]
struct WriteLaneTelemetry {
    enqueue_attempts: AtomicU64,
    enqueue_rejections: AtomicU64,
    depth_samples: AtomicU64,
    saturated_samples: AtomicU64,
}

#[derive(Debug, Clone, Copy, Default)]
struct WriteLaneTelemetrySnapshot {
    enqueue_attempts: u64,
    enqueue_rejections: u64,
    depth_samples: u64,
    saturated_samples: u64,
}

impl WriteLaneTelemetrySnapshot {
    fn merge(self, other: Self) -> Self {
        Self {
            enqueue_attempts: self.enqueue_attempts.saturating_add(other.enqueue_attempts),
            enqueue_rejections: self
                .enqueue_rejections
                .saturating_add(other.enqueue_rejections),
            depth_samples: self.depth_samples.saturating_add(other.depth_samples),
            saturated_samples: self
                .saturated_samples
                .saturating_add(other.saturated_samples),
        }
    }
}

impl WriteLaneTelemetry {
    fn snapshot(&self) -> WriteLaneTelemetrySnapshot {
        WriteLaneTelemetrySnapshot {
            enqueue_attempts: self.enqueue_attempts.load(Ordering::Relaxed),
            enqueue_rejections: self.enqueue_rejections.load(Ordering::Relaxed),
            depth_samples: self.depth_samples.load(Ordering::Relaxed),
            saturated_samples: self.saturated_samples.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone)]
enum WriteEnvelope {
    Put {
        namespace: Bytes,
        key: Bytes,
        value: Bytes,
        expected_version: Option<u64>,
        replication_mode: ReplicationCommitMode,
        ownership_fence: OwnershipFence,
    },
    ClientBatch {
        ops: Vec<BatchOp>,
        replication_mode: ReplicationCommitMode,
        ownership_fence: OwnershipFence,
    },
    TxnCommit {
        txn_id: u64,
    },
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WriteEnvelopeKind {
    Put,
    ClientBatch,
    TxnCommit,
}

#[derive(Debug, Clone)]
struct WriteEnvelopeMessage {
    envelope: WriteEnvelope,
    #[cfg(test)]
    kind: WriteEnvelopeKind,
    logical_shard: u32,
    ops_hint: usize,
    bytes_hint: usize,
    oversize_atomic: bool,
    enqueued_at: Instant,
    response_tx: Sender<Result<WriteResult, DbError>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WriteResult {
    Version(u64),
    TxnCommitted,
}

#[cfg(test)]
#[derive(Debug, Clone)]
struct WriterCommittedGroup {
    kinds: Vec<WriteEnvelopeKind>,
    logical_shards: Vec<u32>,
}

#[derive(Debug)]
struct WriteLaneState {
    queue: VecDeque<WriteEnvelopeMessage>,
    stop: bool,
    #[cfg(test)]
    committed_groups: Vec<WriterCommittedGroup>,
}

#[derive(Debug)]
struct WriteLaneShared {
    state: Mutex<WriteLaneState>,
    cv: Condvar,
    tuning: WriteFlushTuning,
}

#[derive(Debug)]
struct WriteLane {
    lane_id: usize,
    shared: Arc<WriteLaneShared>,
    telemetry: Arc<WriteLaneTelemetry>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Debug)]
struct WriteLanePool {
    lanes: Vec<Arc<WriteLane>>,
    shard_lane_map: RwLock<HashMap<u32, usize>>,
    lane_assigned_shards: Vec<AtomicU64>,
    next_assignment_hint: AtomicUsize,
    lane_assignment_lookups: AtomicU64,
    lane_assignment_hits: AtomicU64,
    lane_assignment_misses: AtomicU64,
}

#[derive(Debug)]
struct ApplyTask {
    active_group_id: u32,
    required_index: u64,
    staged_ops: Vec<StagedApplyOp>,
}

#[derive(Debug)]
struct ApplyLaneState {
    queue: VecDeque<ApplyTask>,
    stop: bool,
}

#[derive(Debug)]
struct ApplyLaneShared {
    state: Mutex<ApplyLaneState>,
    cv: Condvar,
}

#[derive(Debug)]
struct ApplyLane {
    lane_id: usize,
    shared: Arc<ApplyLaneShared>,
    telemetry: Arc<ApplyLaneTelemetry>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Debug)]
struct ApplyLanePool {
    lanes: Vec<Arc<ApplyLane>>,
}

#[derive(Debug, Default)]
struct ApplyLaneTelemetry {
    enqueue_attempts: AtomicU64,
    depth_samples: AtomicU64,
    max_queue_depth: AtomicU64,
    dequeued_tasks: AtomicU64,
}

#[derive(Debug, Clone, Copy, Default)]
struct ApplyLaneTelemetrySnapshot {
    enqueue_attempts: u64,
    depth_samples: u64,
    max_queue_depth: u64,
    dequeued_tasks: u64,
}

#[derive(Debug)]
struct AutoscaleLane {
    stop: Arc<AtomicBool>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

fn now_epoch_s() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|dur| dur.as_secs())
        .unwrap_or(0)
}

fn now_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|dur| dur.as_millis() as u64)
        .unwrap_or(0)
}

fn duration_to_nanos(duration: Duration) -> u64 {
    duration.as_nanos().min(u64::MAX as u128) as u64
}

fn payload_hash64(payload: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    payload.hash(&mut hasher);
    hasher.finish()
}

fn update_max_atomic(target: &AtomicU64, candidate: u64) {
    let mut current = target.load(Ordering::Relaxed);
    while candidate > current {
        match target.compare_exchange_weak(current, candidate, Ordering::Relaxed, Ordering::Relaxed)
        {
            Ok(_) => return,
            Err(observed) => current = observed,
        }
    }
}

fn block_on_runtime<F>(f: F) -> F::Output
where
    F: Future + Send,
    F::Output: Send,
{
    let rt = runtime::tokio_runtime();
    if tokio::runtime::Handle::try_current().is_ok() {
        tokio::task::block_in_place(|| rt.block_on(f))
    } else {
        rt.block_on(f)
    }
}

type RegionAzNodeMap = BTreeMap<String, BTreeMap<String, Vec<String>>>;

fn normalize_region_id(raw: &str) -> Option<String> {
    let normalized = raw.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn normalize_region_list(values: &[String]) -> Vec<String> {
    values
        .iter()
        .filter_map(|value| normalize_region_id(value))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn canonicalize_topology_region_az_node_map(input: &RegionAzNodeMap) -> RegionAzNodeMap {
    let mut canonical_sets: BTreeMap<String, BTreeMap<String, BTreeSet<String>>> = BTreeMap::new();
    for (region, az_map) in input {
        let Some(normalized_region) = normalize_region_id(region) else {
            continue;
        };
        let region_entry = canonical_sets.entry(normalized_region).or_default();
        for (az, nodes) in az_map {
            let Some(normalized_az) = normalize_region_id(az) else {
                continue;
            };
            let az_nodes = region_entry.entry(normalized_az).or_default();
            for node in nodes {
                let node = node.trim();
                if !node.is_empty() {
                    az_nodes.insert(node.to_ascii_lowercase());
                }
            }
        }
    }
    let mut canonical: RegionAzNodeMap = BTreeMap::new();
    for (region, az_sets) in canonical_sets {
        let mut canonical_az_map: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (az, nodes) in az_sets {
            if nodes.is_empty() {
                continue;
            }
            canonical_az_map.insert(az, nodes.into_iter().collect());
        }
        if !canonical_az_map.is_empty() {
            canonical.insert(region, canonical_az_map);
        }
    }
    canonical
}

fn canonical_region_set_from_map(region_az_node_map: &RegionAzNodeMap) -> BTreeSet<String> {
    region_az_node_map.keys().cloned().collect()
}

fn normalize_healthy_nodes(mut nodes: Vec<u64>) -> Vec<u64> {
    nodes.retain(|node| *node != 0);
    if !nodes.contains(&LOCAL_NODE_ID) {
        nodes.push(LOCAL_NODE_ID);
    }
    nodes.sort_unstable();
    nodes.dedup();
    if nodes.is_empty() {
        vec![LOCAL_NODE_ID]
    } else {
        nodes
    }
}

fn healthy_nodes_from_count_env() -> Option<Vec<u64>> {
    let count = std::env::var("WRELADB_HEALTHY_NODE_COUNT")
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?;
    if count == 0 {
        return None;
    }
    Some((1..=count).collect())
}

fn healthy_nodes_from_names_env() -> Option<Vec<u64>> {
    let raw = std::env::var("WRELADB_HEALTHY_NODES").ok()?;
    let mut names = raw
        .split(',')
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    if names.is_empty() {
        return None;
    }
    Some((1..=names.len() as u64).collect())
}

fn fly_machine_region(machine: &JsonValue) -> Option<String> {
    machine
        .get("region")
        .or_else(|| machine.get("Region"))
        .or_else(|| {
            machine
                .get("config")
                .and_then(|config| config.get("region"))
        })
        .or_else(|| {
            machine
                .get("Config")
                .and_then(|config| config.get("Region"))
        })
        .and_then(JsonValue::as_str)
        .map(|region| region.trim().to_ascii_lowercase())
        .filter(|region| !region.is_empty())
}

fn fly_machine_state(machine: &JsonValue) -> Option<String> {
    machine
        .get("state")
        .or_else(|| machine.get("State"))
        .and_then(JsonValue::as_str)
        .map(|state| state.trim().to_ascii_lowercase())
        .filter(|state| !state.is_empty())
}

fn healthy_nodes_from_fly_api(local_region: &str) -> Option<Vec<u64>> {
    let app_name = std::env::var("FLY_APP_NAME").ok()?;
    let token = std::env::var("WRELADB_FLY_API_TOKEN")
        .or_else(|_| std::env::var("FLY_API_TOKEN"))
        .ok()?;
    let app_name = app_name.trim();
    let token = token.trim();
    if app_name.is_empty() || token.is_empty() {
        return None;
    }

    let target_region = normalize_region_id(local_region).filter(|region| region != "local");
    let url = format!("https://api.machines.dev/v1/apps/{app_name}/machines");
    let token = token.to_string();
    let body = runtime::tokio_runtime().block_on(async {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .ok()?;
        let response = client
            .get(&url)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await
            .ok()?;
        response.text().await.ok()
    })?;
    let parsed = serde_json::from_str::<JsonValue>(&body).ok()?;
    let machines = parsed.as_array()?;

    let mut healthy_count = 0u64;
    for machine in machines {
        let state = fly_machine_state(machine).unwrap_or_else(|| "unknown".to_string());
        if state != "started" && state != "starting" {
            continue;
        }
        if let Some(target) = target_region.as_ref()
            && let Some(region) = fly_machine_region(machine)
            && &region != target
        {
            continue;
        }
        healthy_count = healthy_count.saturating_add(1);
    }
    if healthy_count == 0 {
        return None;
    }
    Some((1..=healthy_count).collect())
}

fn discover_healthy_nodes_for_background(local_region: &str) -> Vec<u64> {
    if let Some(nodes) = healthy_nodes_from_count_env() {
        return normalize_healthy_nodes(nodes);
    }
    if let Some(nodes) = healthy_nodes_from_names_env() {
        return normalize_healthy_nodes(nodes);
    }
    if let Some(nodes) = healthy_nodes_from_fly_api(local_region) {
        return normalize_healthy_nodes(nodes);
    }
    vec![LOCAL_NODE_ID]
}

fn default_membership_for_replication_factor(
    replication_factor: u32,
) -> Result<MembershipConfig, DbError> {
    let voters = (1..=replication_factor.max(1))
        .map(u64::from)
        .collect::<Vec<_>>();
    MembershipConfig::new(voters)
        .map_err(|err| DbError::invalid_argument(format!("membership init failed: {err:?}")))
}

impl AutoscaleLane {
    fn start(
        handle: i64,
        engine: Arc<RwLock<DbEngine>>,
        tick_ms: u64,
        local_region: String,
    ) -> Result<Arc<Self>, DbError> {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let thread_engine = engine.clone();
        let sleep_window = Duration::from_millis(tick_ms.max(100));
        let sleep_slice = Duration::from_millis(100);
        let mut cached_healthy_nodes = vec![LOCAL_NODE_ID];
        let mut last_discovery = Instant::now()
            .checked_sub(Duration::from_secs(5))
            .unwrap_or_else(Instant::now);
        let discovery_interval = Duration::from_secs(5);
        let name = format!("wrela-db-autoscale-{handle}");
        let worker = thread::Builder::new()
            .name(name)
            .spawn(move || {
                let mut accumulated = Duration::ZERO;
                while !thread_stop.load(Ordering::Relaxed) {
                    let remaining = sleep_window.saturating_sub(accumulated);
                    let wait = remaining.min(sleep_slice);
                    thread::sleep(wait);
                    accumulated = accumulated.saturating_add(wait);
                    if thread_stop.load(Ordering::Relaxed) {
                        break;
                    }
                    if accumulated >= sleep_window {
                        if last_discovery.elapsed() >= discovery_interval {
                            cached_healthy_nodes =
                                discover_healthy_nodes_for_background(&local_region);
                            last_discovery = Instant::now();
                        }
                        if let Ok(mut db) = thread_engine.write() {
                            db.checkpoint_tick_background();
                            db.run_autopilot_controller_tick("background");
                            let _ = db
                                .autoscale_tick_background_with_nodes(cached_healthy_nodes.clone());
                        } else {
                            break;
                        }
                        accumulated = Duration::ZERO;
                    }
                }
            })
            .map_err(|err| DbError::io(format!("failed to spawn autoscale lane: {err}")))?;
        Ok(Arc::new(Self {
            stop,
            thread: Mutex::new(Some(worker)),
        }))
    }

    fn shutdown(&self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Ok(mut guard) = self.thread.lock()
            && let Some(worker) = guard.take()
        {
            let _ = worker.join();
        }
    }
}

fn envelope_batch_weight(ops: &[BatchOp]) -> (usize, usize) {
    let mut total_bytes = 0usize;
    for op in ops {
        match op {
            BatchOp::Put {
                namespace,
                key,
                value,
                ..
            } => {
                total_bytes = total_bytes
                    .saturating_add(namespace.len())
                    .saturating_add(key.len())
                    .saturating_add(value.len());
            }
            BatchOp::Delete { namespace, key, .. } => {
                total_bytes = total_bytes
                    .saturating_add(namespace.len())
                    .saturating_add(key.len());
            }
        }
    }
    (ops.len(), total_bytes)
}

fn oversize_atomic_for(can_be_oversize_atomic: bool, ops_hint: usize, bytes_hint: usize) -> bool {
    let tuning = WriteFlushTuning::default();
    can_be_oversize_atomic && (ops_hint > tuning.max_ops || bytes_hint > tuning.soft_bytes)
}

impl WriteLane {
    fn start(
        handle: i64,
        lane_id: usize,
        engine: Arc<RwLock<DbEngine>>,
    ) -> Result<Arc<Self>, DbError> {
        let tuning = WriteFlushTuning::default();
        let telemetry = Arc::new(WriteLaneTelemetry::default());
        let shared = Arc::new(WriteLaneShared {
            state: Mutex::new(WriteLaneState {
                queue: VecDeque::new(),
                stop: false,
                #[cfg(test)]
                committed_groups: Vec::new(),
            }),
            cv: Condvar::new(),
            tuning,
        });
        let thread_shared = shared.clone();
        let thread_engine = engine.clone();
        let name = format!("wrela-db-writer-{handle}-{lane_id}");
        let thread_lane_id = lane_id;
        let worker = thread::Builder::new()
            .name(name)
            .spawn(move || writer_lane_loop(thread_shared, thread_engine, handle, thread_lane_id))
            .map_err(|err| DbError::io(format!("failed to spawn writer lane: {err}")))?;
        Ok(Arc::new(Self {
            lane_id,
            shared,
            telemetry,
            thread: Mutex::new(Some(worker)),
        }))
    }

    fn enqueue(&self, message: WriteEnvelopeMessage) -> Result<(), DbError> {
        self.telemetry
            .enqueue_attempts
            .fetch_add(1, Ordering::Relaxed);
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| DbError::io("write lane lock poisoned"))?;
        if state.stop {
            return Err(DbError::invalid_argument("unknown DB handle"));
        }
        let queue_len = state.queue.len();
        self.telemetry.depth_samples.fetch_add(1, Ordering::Relaxed);
        if queue_len >= WriteFlushTuning::saturation_threshold() {
            self.telemetry
                .saturated_samples
                .fetch_add(1, Ordering::Relaxed);
        }
        if queue_len >= WriteFlushTuning::capacity() {
            self.telemetry
                .enqueue_rejections
                .fetch_add(1, Ordering::Relaxed);
            return Err(DbError::limit(
                "detached writer queue saturated; RETRY_AFTER_MS=25",
            ));
        }
        state.queue.push_back(message);
        self.shared.cv.notify_one();
        Ok(())
    }

    fn shutdown(&self) {
        if let Ok(mut state) = self.shared.state.lock() {
            state.stop = true;
            self.shared.cv.notify_all();
        }
        if let Ok(mut guard) = self.thread.lock()
            && let Some(worker) = guard.take()
        {
            let _ = worker.join();
        }
    }

    #[cfg(test)]
    fn committed_groups(&self) -> Vec<WriterCommittedGroup> {
        self.shared
            .state
            .lock()
            .expect("write lane lock")
            .committed_groups
            .clone()
    }

    fn telemetry_snapshot(&self) -> WriteLaneTelemetrySnapshot {
        self.telemetry.snapshot()
    }

    fn status(&self, assigned_shards: u64) -> DbWriterLaneStatus {
        let queue_depth = self
            .shared
            .state
            .lock()
            .map(|state| state.queue.len() as u64)
            .unwrap_or(0);
        let telemetry = self.telemetry_snapshot();
        DbWriterLaneStatus {
            lane_id: self.lane_id,
            assigned_shards,
            queue_depth,
            enqueue_attempts: telemetry.enqueue_attempts,
            enqueue_rejections: telemetry.enqueue_rejections,
            depth_samples: telemetry.depth_samples,
            saturated_samples: telemetry.saturated_samples,
        }
    }
}

impl WriteLanePool {
    fn start(
        handle: i64,
        engine: Arc<RwLock<DbEngine>>,
        lane_count: usize,
    ) -> Result<Arc<Self>, DbError> {
        let lane_count = lane_count.max(1);
        let mut lanes = Vec::with_capacity(lane_count);
        for lane_id in 0..lane_count {
            match WriteLane::start(handle, lane_id, engine.clone()) {
                Ok(lane) => lanes.push(lane),
                Err(err) => {
                    for lane in &lanes {
                        lane.shutdown();
                    }
                    return Err(err);
                }
            }
        }
        let lane_assigned_shards = (0..lane_count)
            .map(|_| AtomicU64::new(0))
            .collect::<Vec<_>>();
        Ok(Arc::new(Self {
            lanes,
            shard_lane_map: RwLock::new(HashMap::new()),
            lane_assigned_shards,
            next_assignment_hint: AtomicUsize::new(0),
            lane_assignment_lookups: AtomicU64::new(0),
            lane_assignment_hits: AtomicU64::new(0),
            lane_assignment_misses: AtomicU64::new(0),
        }))
    }

    fn select_lane_for_new_shard(&self, logical_shard: u32) -> usize {
        let lane_count = self.lanes.len().max(1);
        if lane_count == 1 {
            return 0;
        }
        let preferred = (logical_shard as usize) % lane_count;
        let start = self.next_assignment_hint.fetch_add(1, Ordering::Relaxed) % lane_count;
        let mut best_idx = start;
        let mut best_assigned = self.lane_assigned_shards[start].load(Ordering::Relaxed);
        for offset in 1..lane_count {
            let idx = (start + offset) % lane_count;
            let assigned = self.lane_assigned_shards[idx].load(Ordering::Relaxed);
            let better = assigned < best_assigned
                || (assigned == best_assigned && idx == preferred && best_idx != preferred);
            if better {
                best_idx = idx;
                best_assigned = assigned;
            }
        }
        best_idx
    }

    fn lane_for_shard(&self, logical_shard: u32) -> Arc<WriteLane> {
        let lane_count = self.lanes.len().max(1);
        if lane_count == 1 {
            return self.lanes[0].clone();
        }
        self.lane_assignment_lookups.fetch_add(1, Ordering::Relaxed);
        if let Ok(guard) = self.shard_lane_map.read()
            && let Some(lane_idx) = guard.get(&logical_shard).copied()
        {
            self.lane_assignment_hits.fetch_add(1, Ordering::Relaxed);
            return self.lanes[lane_idx].clone();
        }
        self.lane_assignment_misses.fetch_add(1, Ordering::Relaxed);

        let candidate = self.select_lane_for_new_shard(logical_shard);
        if let Ok(mut guard) = self.shard_lane_map.write() {
            let lane_idx = *guard.entry(logical_shard).or_insert_with(|| {
                self.lane_assigned_shards[candidate].fetch_add(1, Ordering::Relaxed);
                candidate
            });
            return self.lanes[lane_idx].clone();
        }
        self.lanes[candidate].clone()
    }

    fn assignment_stats(&self) -> (u64, u64, u64) {
        (
            self.lane_assignment_lookups.load(Ordering::Relaxed),
            self.lane_assignment_hits.load(Ordering::Relaxed),
            self.lane_assignment_misses.load(Ordering::Relaxed),
        )
    }

    fn telemetry_snapshot(&self) -> WriteLaneTelemetrySnapshot {
        self.lanes
            .iter()
            .fold(WriteLaneTelemetrySnapshot::default(), |acc, lane| {
                acc.merge(lane.telemetry_snapshot())
            })
    }

    fn shutdown(&self) {
        for lane in &self.lanes {
            lane.shutdown();
        }
    }

    #[cfg(test)]
    fn committed_groups(&self) -> Vec<WriterCommittedGroup> {
        let mut groups = Vec::new();
        for lane in &self.lanes {
            groups.extend(lane.committed_groups());
        }
        groups
    }

    fn statuses(&self) -> Vec<DbWriterLaneStatus> {
        let mut statuses = self
            .lanes
            .iter()
            .enumerate()
            .map(|(idx, lane)| lane.status(self.lane_assigned_shards[idx].load(Ordering::Relaxed)))
            .collect::<Vec<_>>();
        statuses.sort_by_key(|status| status.lane_id);
        statuses
    }

    fn lane_count(&self) -> usize {
        self.lanes.len().max(1)
    }
}

impl ApplyLane {
    fn start(
        handle: i64,
        lane_id: usize,
        engine: Arc<RwLock<DbEngine>>,
    ) -> Result<Arc<Self>, DbError> {
        let telemetry = Arc::new(ApplyLaneTelemetry::default());
        let shared = Arc::new(ApplyLaneShared {
            state: Mutex::new(ApplyLaneState {
                queue: VecDeque::new(),
                stop: false,
            }),
            cv: Condvar::new(),
        });
        let thread_shared = shared.clone();
        let thread_telemetry = telemetry.clone();
        let thread_engine = engine.clone();
        let name = format!("wrela-db-apply-{handle}-{lane_id}");
        let worker = thread::Builder::new()
            .name(name)
            .spawn(move || apply_lane_loop(thread_shared, thread_telemetry, thread_engine))
            .map_err(|err| DbError::io(format!("failed to spawn apply lane: {err}")))?;
        Ok(Arc::new(Self {
            lane_id,
            shared,
            telemetry,
            thread: Mutex::new(Some(worker)),
        }))
    }

    fn enqueue(&self, task: ApplyTask) -> Result<(), ApplyTask> {
        self.telemetry
            .enqueue_attempts
            .fetch_add(1, Ordering::Relaxed);
        let mut state = match self.shared.state.lock() {
            Ok(state) => state,
            Err(_) => return Err(task),
        };
        if state.stop {
            return Err(task);
        }
        self.telemetry.depth_samples.fetch_add(1, Ordering::Relaxed);
        let queued_after = state.queue.len().saturating_add(1) as u64;
        update_max_atomic(&self.telemetry.max_queue_depth, queued_after);
        state.queue.push_back(task);
        self.shared.cv.notify_one();
        Ok(())
    }

    fn shutdown(&self) {
        if let Ok(mut state) = self.shared.state.lock() {
            state.stop = true;
            self.shared.cv.notify_all();
        }
        if let Ok(mut guard) = self.thread.lock()
            && let Some(worker) = guard.take()
        {
            let _ = worker.join();
        }
    }

    fn telemetry_snapshot(&self) -> ApplyLaneTelemetrySnapshot {
        ApplyLaneTelemetrySnapshot {
            enqueue_attempts: self.telemetry.enqueue_attempts.load(Ordering::Relaxed),
            depth_samples: self.telemetry.depth_samples.load(Ordering::Relaxed),
            max_queue_depth: self.telemetry.max_queue_depth.load(Ordering::Relaxed),
            dequeued_tasks: self.telemetry.dequeued_tasks.load(Ordering::Relaxed),
        }
    }

    fn status(&self) -> DbApplyLaneStatus {
        let queue_depth = self
            .shared
            .state
            .lock()
            .map(|state| state.queue.len() as u64)
            .unwrap_or(0);
        let telemetry = self.telemetry_snapshot();
        DbApplyLaneStatus {
            lane_id: self.lane_id,
            queue_depth,
            enqueue_attempts: telemetry.enqueue_attempts,
            depth_samples: telemetry.depth_samples,
            max_queue_depth: telemetry.max_queue_depth,
            dequeued_tasks: telemetry.dequeued_tasks,
        }
    }
}

impl ApplyLanePool {
    fn start(
        handle: i64,
        engine: Arc<RwLock<DbEngine>>,
        lane_count: usize,
    ) -> Result<Arc<Self>, DbError> {
        let lane_count = lane_count.max(1);
        let mut lanes = Vec::with_capacity(lane_count);
        for lane_id in 0..lane_count {
            match ApplyLane::start(handle, lane_id, engine.clone()) {
                Ok(lane) => lanes.push(lane),
                Err(err) => {
                    for lane in &lanes {
                        lane.shutdown();
                    }
                    return Err(err);
                }
            }
        }
        Ok(Arc::new(Self { lanes }))
    }

    fn lane_for_group(&self, active_group_id: u32) -> Arc<ApplyLane> {
        if self.lanes.len() == 1 {
            return self.lanes[0].clone();
        }
        let lane_idx = (active_group_id as usize) % self.lanes.len();
        self.lanes[lane_idx].clone()
    }

    fn enqueue(&self, task: ApplyTask) -> Result<(), ApplyTask> {
        self.lane_for_group(task.active_group_id).enqueue(task)
    }

    fn shutdown(&self) {
        for lane in &self.lanes {
            lane.shutdown();
        }
    }

    fn statuses(&self) -> Vec<DbApplyLaneStatus> {
        let mut statuses = self
            .lanes
            .iter()
            .map(|lane| lane.status())
            .collect::<Vec<_>>();
        statuses.sort_by_key(|status| status.lane_id);
        statuses
    }

    fn lane_count(&self) -> usize {
        self.lanes.len().max(1)
    }
}

fn apply_lane_loop(
    shared: Arc<ApplyLaneShared>,
    telemetry: Arc<ApplyLaneTelemetry>,
    engine: Arc<RwLock<DbEngine>>,
) {
    let apply_batch_max = APPLY_LANE_BATCH_MAX.max(1);
    loop {
        let mut tasks = {
            let mut state = match shared.state.lock() {
                Ok(state) => state,
                Err(_) => return,
            };
            while state.queue.is_empty() && !state.stop {
                state = match shared.cv.wait(state) {
                    Ok(guard) => guard,
                    Err(_) => return,
                };
            }
            if state.stop && state.queue.is_empty() {
                return;
            }
            let mut drained = Vec::with_capacity(apply_batch_max.min(state.queue.len().max(1)));
            if let Some(task) = state.queue.pop_front() {
                drained.push(task);
            }
            while drained.len() < apply_batch_max {
                let Some(task) = state.queue.pop_front() else {
                    break;
                };
                drained.push(task);
            }
            drained
        };
        if tasks.is_empty() {
            continue;
        }
        telemetry
            .dequeued_tasks
            .fetch_add(tasks.len() as u64, Ordering::Relaxed);
        if let Ok(mut db) = engine.write() {
            for task in tasks.drain(..) {
                db.apply_committed_task(task);
            }
        } else {
            return;
        }
    }
}

fn wal_completion_disconnected_error() -> io::Error {
    io::Error::other("WAL flush coordinator dropped completion")
}

fn wal_completion_timeout_error(timeout: Duration) -> io::Error {
    io::Error::new(
        io::ErrorKind::TimedOut,
        format!(
            "WAL completion timeout while waiting for writer lane result ({} ms)",
            timeout.as_millis()
        ),
    )
}

fn poll_wal_completion_receivers(
    rx_list: &[WalCompletionReceiver<io::Result<WalBatchCompletion>>],
    completion_results: &mut [Option<io::Result<WalBatchCompletion>>],
) -> bool {
    let mut all_ready = true;
    for (idx, rx) in rx_list.iter().enumerate() {
        if completion_results[idx].is_some() {
            continue;
        }
        match rx.try_recv() {
            Ok(result) => completion_results[idx] = Some(result),
            Err(TryRecvError::Empty) => all_ready = false,
            Err(TryRecvError::Disconnected) => {
                completion_results[idx] = Some(Err(wal_completion_disconnected_error()));
            }
        }
    }
    all_ready && completion_results.iter().all(Option::is_some)
}

fn drain_wal_completions_blocking(
    rx_list: &[WalCompletionReceiver<io::Result<WalBatchCompletion>>],
    completion_results: &mut [Option<io::Result<WalBatchCompletion>>],
    group_dequeued_at: Instant,
    timeout: Duration,
) -> Vec<io::Result<WalBatchCompletion>> {
    let mut completions = Vec::with_capacity(rx_list.len());
    for (idx, rx) in rx_list.iter().enumerate() {
        if let Some(result) = completion_results[idx].take() {
            completions.push(result);
            continue;
        }
        let elapsed = group_dequeued_at.elapsed();
        let remaining = timeout.saturating_sub(elapsed);
        if remaining.is_zero() {
            completions.push(Err(wal_completion_timeout_error(timeout)));
            continue;
        }
        match rx.recv_timeout(remaining) {
            Ok(result) => completions.push(result),
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                completions.push(Err(wal_completion_timeout_error(timeout)));
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                completions.push(Err(wal_completion_disconnected_error()));
            }
        }
    }
    completions
}

fn writer_lane_loop(
    shared: Arc<WriteLaneShared>,
    engine: Arc<RwLock<DbEngine>>,
    handle: i64,
    lane_id: usize,
) {
    let mut pending_groups: VecDeque<PendingWalGroup> = VecDeque::new();
    let mut pending_replications: VecDeque<InFlightReplication> = VecDeque::new();
    let wal_completion_timeout = WAL_COMPLETION_TIMEOUT;
    let shard_ops_accum = match engine.read() {
        Ok(db) => Arc::clone(&db.shard_write_ops_accum),
        Err(_) => return,
    };
    let mesh = mesh_for_handle(handle).ok().flatten();

    /// Drain the oldest completed replications, finalizing them into WAL-pending
    /// groups. Stops at the first replication that hasn't finished yet.
    fn drain_completed_replications(
        pending_replications: &mut VecDeque<InFlightReplication>,
        pending_groups: &mut VecDeque<PendingWalGroup>,
        engine: &Arc<RwLock<DbEngine>>,
        handle: i64,
        shard_ops_accum: &Arc<Mutex<HashMap<u32, u64>>>,
    ) {
        while let Some(front) = pending_replications.front() {
            match front.result_rx.try_recv() {
                Ok(fanout_result) => {
                    let inflight = pending_replications.pop_front().unwrap();
                    if let Some(pending) = finalize_inflight_replication(
                        engine,
                        shard_ops_accum,
                        inflight,
                        fanout_result,
                    ) {
                        if pending.rx_list.is_empty() {
                            finish_pending_wal_group(engine, handle, pending, vec![]);
                        } else {
                            pending_groups.push_back(pending);
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    let inflight = pending_replications.pop_front().unwrap();
                    // Replication thread panicked; report error to callers.
                    for (msg, result) in inflight.group.iter().zip(
                        inflight
                            .per_message_results
                            .into_iter()
                            .map(|r| r.or_else(|_| Err(DbError::io("replication thread lost")))),
                    ) {
                        let _ = msg.response_tx.send(result);
                    }
                }
            }
        }
    }

    /// Block-wait for the oldest in-flight replication to finish.
    fn block_oldest_replication(
        pending_replications: &mut VecDeque<InFlightReplication>,
        pending_groups: &mut VecDeque<PendingWalGroup>,
        engine: &Arc<RwLock<DbEngine>>,
        handle: i64,
        shard_ops_accum: &Arc<Mutex<HashMap<u32, u64>>>,
    ) {
        if let Some(inflight) = pending_replications.pop_front() {
            let fanout_result = inflight.result_rx.recv().unwrap_or_else(|_| {
                Err(OutsideLockReplicationError {
                    token: "REPLICATION_PIPELINE_RECV_FAILED",
                    detail: "replication pipeline channel disconnected".into(),
                    total_target_count: 0,
                    contacted_target_count: 0,
                    replication_wave_count: 0,
                    replication_wave_total_targets: 0,
                    replication_wave_max_targets: 0,
                    successful_target_count: 0,
                    failed_target_count: 0,
                    cancelled_target_count: 0,
                    aborted_in_flight_count: 0,
                    replication_error: None,
                })
            });
            if let Some(pending) =
                finalize_inflight_replication(engine, shard_ops_accum, inflight, fanout_result)
            {
                if pending.rx_list.is_empty() {
                    finish_pending_wal_group(engine, handle, pending, vec![]);
                } else {
                    pending_groups.push_back(pending);
                }
            }
        }
    }

    loop {
        // Drain completed replications → finalize → push to pending_groups.
        drain_completed_replications(
            &mut pending_replications,
            &mut pending_groups,
            &engine,
            handle,
            &shard_ops_accum,
        );

        // Drain completed pending WAL groups (FIFO).
        while !pending_groups.is_empty() {
            let all_ready = {
                let front = pending_groups.front_mut().expect("pending group");
                let ready =
                    poll_wal_completion_receivers(&front.rx_list, &mut front.completion_results);
                if ready {
                    true
                } else if front.group_dequeued_at.elapsed() >= wal_completion_timeout {
                    for slot in &mut front.completion_results {
                        if slot.is_none() {
                            *slot = Some(Err(wal_completion_timeout_error(wal_completion_timeout)));
                        }
                    }
                    true
                } else {
                    false
                }
            };
            if !all_ready {
                break;
            }
            let mut pending = pending_groups.pop_front().expect("pending group");
            let completions = drain_wal_completions_blocking(
                &pending.rx_list,
                &mut pending.completion_results,
                pending.group_dequeued_at,
                wal_completion_timeout,
            );
            finish_pending_wal_group(&engine, handle, pending, completions);
        }

        // If replication pipeline is full, block on the oldest before accepting more work.
        if pending_replications.len() >= REPLICATION_PIPELINE_DEPTH {
            block_oldest_replication(
                &mut pending_replications,
                &mut pending_groups,
                &engine,
                handle,
                &shard_ops_accum,
            );
        }

        // Get next group. If queue empty and we have pending, block on oldest rx.
        let group = {
            let mut state = match shared.state.lock() {
                Ok(s) => s,
                Err(_) => return,
            };

            if state.queue.is_empty() {
                if state.stop {
                    // Drain pending replications before exiting.
                    drop(state);
                    while !pending_replications.is_empty() {
                        block_oldest_replication(
                            &mut pending_replications,
                            &mut pending_groups,
                            &engine,
                            handle,
                            &shard_ops_accum,
                        );
                    }
                    if pending_groups.is_empty() {
                        return;
                    }
                    if let Some(mut pending) = pending_groups.pop_front() {
                        let completions = drain_wal_completions_blocking(
                            &pending.rx_list,
                            &mut pending.completion_results,
                            pending.group_dequeued_at,
                            wal_completion_timeout,
                        );
                        finish_pending_wal_group(&engine, handle, pending, completions);
                    }
                    continue;
                }
                if !pending_replications.is_empty() {
                    // No queued work but replication in-flight; block on it.
                    drop(state);
                    block_oldest_replication(
                        &mut pending_replications,
                        &mut pending_groups,
                        &engine,
                        handle,
                        &shard_ops_accum,
                    );
                    continue;
                }
                if let Some(mut front) = pending_groups.pop_front() {
                    drop(state);
                    let completions = drain_wal_completions_blocking(
                        &front.rx_list,
                        &mut front.completion_results,
                        front.group_dequeued_at,
                        wal_completion_timeout,
                    );
                    finish_pending_wal_group(&engine, handle, front, completions);
                    continue;
                }
            }

            while state.queue.is_empty() && !state.stop {
                state = match shared.cv.wait(state) {
                    Ok(s) => s,
                    Err(_) => return,
                };
            }
            if state.stop && state.queue.is_empty() {
                if pending_groups.is_empty() && pending_replications.is_empty() {
                    return;
                }
                drop(state);
                continue;
            }
            let Some(first) = state.queue.pop_front() else {
                continue;
            };
            if first.oversize_atomic {
                vec![first]
            } else {
                let queue_depth = state.queue.len().saturating_add(1);
                let shard_local_depth = state
                    .queue
                    .iter()
                    .filter(|candidate| {
                        !candidate.oversize_atomic && candidate.logical_shard == first.logical_shard
                    })
                    .count()
                    .saturating_add(1);
                let tuning = shared
                    .tuning
                    .dynamic_for_queue(queue_depth, shard_local_depth);
                let mut group = vec![first];
                let mut ops_hint = group[0].ops_hint;
                let mut bytes_hint = group[0].bytes_hint;
                let group_shard = group[0].logical_shard;
                let started = Instant::now();
                loop {
                    let elapsed = started.elapsed();
                    if elapsed >= tuning.window
                        || ops_hint >= tuning.max_ops
                        || bytes_hint >= tuning.soft_bytes
                    {
                        break;
                    }

                    let mut found_any = false;
                    let mut remaining = VecDeque::with_capacity(state.queue.len());
                    while let Some(next) = state.queue.pop_front() {
                        if next.oversize_atomic || next.logical_shard != group_shard {
                            remaining.push_back(next);
                            continue;
                        }
                        let next_ops = ops_hint.saturating_add(next.ops_hint);
                        let next_bytes = bytes_hint.saturating_add(next.bytes_hint);
                        if next_ops > tuning.max_ops || next_bytes > tuning.soft_bytes {
                            remaining.push_back(next);
                            continue;
                        }
                        ops_hint = next_ops;
                        bytes_hint = next_bytes;
                        group.push(next);
                        found_any = true;
                    }
                    state.queue = remaining;
                    if found_any {
                        continue;
                    }

                    if state.stop {
                        break;
                    }
                    let timeout = tuning.window.saturating_sub(elapsed);
                    let waited = shared.cv.wait_timeout(state, timeout);
                    let (new_state, wait_result) = match waited {
                        Ok(v) => v,
                        Err(_) => return,
                    };
                    state = new_state;
                    if wait_result.timed_out() {
                        break;
                    }
                }
                group
            }
        };

        #[cfg(test)]
        let committed_group = WriterCommittedGroup {
            kinds: group.iter().map(|message| message.kind).collect(),
            logical_shards: group.iter().map(|message| message.logical_shard).collect(),
        };
        #[cfg(test)]
        if let Ok(mut state) = shared.state.lock() {
            state.committed_groups.push(committed_group);
        }

        let group_dequeued_at = Instant::now();

        // Try the pipelined path: prepare + WAL submit + spawn replication (non-blocking).
        // Falls back to the blocking path for non-quorum groups or when the mesh is unavailable.
        let group = if let Some(ref mesh_arc) = mesh {
            match try_pipeline_quorum_group(
                &engine,
                handle,
                lane_id,
                &shard_ops_accum,
                &group,
                group_dequeued_at,
                mesh_arc,
            ) {
                Some(mut inflight) => {
                    inflight.group = group;
                    pending_replications.push_back(inflight);
                    continue; // pipelined — proceed to next iteration
                }
                None => group, // not eligible — fall through to blocking path
            }
        } else {
            group
        };

        if let Some(pending) = process_write_group_pipelined(
            &engine,
            handle,
            lane_id,
            &shard_ops_accum,
            group,
            group_dequeued_at,
        ) {
            if pending.rx_list.is_empty() {
                // No WAL submissions (all failed or txn-only); finish immediately.
                finish_pending_wal_group(&engine, handle, pending, vec![]);
            } else {
                pending_groups.push_back(pending);
            }
        }
    }
}

/// A group with WAL submissions in flight. Completions are drained by the writer loop.
struct PendingWalGroup {
    group_dequeued_at: Instant,
    rx_list: Vec<WalCompletionReceiver<io::Result<WalBatchCompletion>>>,
    completion_results: Vec<Option<io::Result<WalBatchCompletion>>>,
    wal_submitted_at: Vec<Instant>,
    apply_results: Vec<PrepareAndApplyResult>,
    msg_maps: Vec<Vec<(usize, usize, usize)>>,
    ops_list: Vec<Vec<BatchOp>>,
    group: Vec<WriteEnvelopeMessage>,
    per_message_results: Vec<Result<WriteResult, DbError>>,
}

/// Maximum number of batches with in-flight replication that a single writer lane
/// can have outstanding. When the limit is reached the lane blocks on the oldest.
const REPLICATION_PIPELINE_DEPTH: usize = 2;

/// A quorum batch whose replication RPC has been spawned on a background thread.
/// The writer lane finalizes it when the result arrives, allowing the lane to
/// prepare and submit WAL for the next batch while replication is in-flight.
struct InFlightReplication {
    group_dequeued_at: Instant,
    result_rx: mpsc::Receiver<Result<OutsideLockFanoutResult, OutsideLockReplicationError>>,
    prepared: Option<PreparedOutsideLockBatch>,
    wal_rx: WalCompletionReceiver<io::Result<WalBatchCompletion>>,
    wal_submitted_at: Instant,
    pre_encode_ns: u64,
    ops: Vec<BatchOp>,
    msg_map: Vec<(usize, usize, usize)>,
    group: Vec<WriteEnvelopeMessage>,
    per_message_results: Vec<Result<WriteResult, DbError>>,
}

/// Spawn replication on a background thread so the writer lane can proceed.
/// Returns a receiver that yields the fanout result.
#[allow(dead_code)]
fn spawn_replication(
    mesh: &Arc<PrivateMeshContext>,
    prepared: &PreparedOutsideLockBatch,
) -> mpsc::Receiver<Result<OutsideLockFanoutResult, OutsideLockReplicationError>> {
    let (tx, rx) = mpsc::channel();
    // Clone the Arc so the thread can reference the mesh.
    let mesh_clone = Arc::clone(mesh);
    // Extract the minimal data the replication function reads from `prepared`.
    // We build a shallow copy of PreparedOutsideLockBatch for the thread.
    let repl_prepared = PreparedOutsideLockBatch {
        active_group_id: prepared.active_group_id,
        required_term: prepared.required_term,
        required_index: prepared.required_index,
        logical_shard_id: prepared.logical_shard_id,
        ownership_fence: prepared.ownership_fence.clone(),
        batch_ops: prepared.batch_ops.clone(),
        membership: prepared.membership.clone(),
        write_quorum_required: prepared.write_quorum_required,
        replica_latency_rank: prepared.replica_latency_rank.clone(),
        follower_progress_hints: prepared.follower_progress_hints.clone(),
        leader_commit: prepared.leader_commit,
        leader_snapshot: prepared.leader_snapshot.clone(),
        follower_snapshots: prepared.follower_snapshots.clone(),
        require_private_rpc_transport: prepared.require_private_rpc_transport,
        simulation_fallback_allowed: prepared.simulation_fallback_allowed,
        staged_records: prepared.staged_records.clone(),
        // Fields only needed for finalization, not replication:
        committed_versions: Vec::new(),
        staged_ops: Vec::new(),
        staged_entries: Vec::new(),
        max_version: 0,
        op_count: 0,
        byte_count: 0,
        queue_wait_ns: 0,
        engine_lock_wait_ns: 0,
        validate_route_ns: 0,
        total_started: Instant::now(),
    };
    std::thread::Builder::new()
        .name("wrela-repl-pipe".into())
        .spawn(move || {
            let result = replicate_prepared_batch_over_private_mesh(&mesh_clone, &repl_prepared);
            let _ = tx.send(result);
        })
        .expect("spawn replication pipeline thread");
    rx
}

/// Finalize an in-flight replication after its result arrives.
/// On success returns a `PendingWalGroup` that the writer lane pushes onto
/// its pending queue for WAL completion draining. On failure, sends error
/// responses directly and returns `None`.
fn finalize_inflight_replication(
    engine: &Arc<RwLock<DbEngine>>,
    shard_ops_accum: &Arc<Mutex<HashMap<u32, u64>>>,
    mut inflight: InFlightReplication,
    fanout_result: Result<OutsideLockFanoutResult, OutsideLockReplicationError>,
) -> Option<PendingWalGroup> {
    let prepared = inflight
        .prepared
        .take()
        .expect("prepared batch must be present");
    let result = match engine.write() {
        Ok(mut db) => db.finalize_prepared_batch_after_outside_replication(prepared, fanout_result),
        Err(_) => Err(DbError::io("DB engine lock poisoned")),
    };
    match result {
        Ok(mut apply_result) => {
            apply_result.encode_ns = inflight.pre_encode_ns;
            apply_result.wal_records.clear();
            if let Some((shard_id, op_count)) = apply_result.shard_ops_delta {
                if let Ok(mut accum) = shard_ops_accum.lock() {
                    let c = accum.entry(shard_id).or_insert(0);
                    *c = c.saturating_add(op_count);
                }
            }
            Some(PendingWalGroup {
                group_dequeued_at: inflight.group_dequeued_at,
                completion_results: vec![None],
                rx_list: vec![inflight.wal_rx],
                wal_submitted_at: vec![inflight.wal_submitted_at],
                apply_results: vec![apply_result],
                msg_maps: vec![inflight.msg_map],
                ops_list: vec![inflight.ops],
                group: inflight.group,
                per_message_results: inflight.per_message_results,
            })
        }
        Err(err) => {
            for (msg_idx, _, _) in &inflight.msg_map {
                inflight.per_message_results[*msg_idx] = Err(err.clone());
            }
            for (msg, result) in inflight
                .group
                .iter()
                .zip(inflight.per_message_results.into_iter())
            {
                let _ = msg.response_tx.send(result);
            }
            None
        }
    }
}

/// Work item produced from messages before the lock is acquired.
/// Batches carry pre-validated, pre-framed data so the DbEngine
/// lock only covers state-dependent work.
enum WriteWorkItem {
    Batch {
        ops: Vec<BatchOp>,
        msg_map: Vec<(usize, usize, usize)>,
        preprocessed: Result<PreProcessedBatch, DbError>,
        replication_mode: ReplicationCommitMode,
        ownership_fence: OwnershipFence,
        queue_wait_ns: u64,
    },
    TxnCommit {
        msg_idx: usize,
        txn_id: u64,
    },
}

/// Single work item or multiple Quorum batches coalesced into one replication RPC.
enum ProcessUnit {
    Single(WriteWorkItem),
    CoalescedQuorum {
        ops: Vec<BatchOp>,
        msg_map: Vec<(usize, usize, usize)>,
        queue_wait_ns: u64,
        preprocessed: Result<PreProcessedBatch, DbError>,
        ownership_fence: OwnershipFence,
    },
}

fn coalesce_quorum_batches(mut work_items: Vec<WriteWorkItem>) -> Vec<ProcessUnit> {
    let mut units = Vec::new();
    while !work_items.is_empty() {
        let item = work_items.remove(0);
        match item {
            WriteWorkItem::TxnCommit { .. } => {
                units.push(ProcessUnit::Single(item));
            }
            WriteWorkItem::Batch {
                replication_mode: ReplicationCommitMode::ReplicaLocal,
                ..
            } => {
                units.push(ProcessUnit::Single(item));
            }
            WriteWorkItem::Batch {
                ops,
                msg_map,
                queue_wait_ns,
                ownership_fence,
                replication_mode: ReplicationCommitMode::Quorum,
                ..
            } => {
                let mut merged_ops = ops;
                let mut merged_map = msg_map;
                let mut max_queue_wait_ns = queue_wait_ns;
                let merged_fence = ownership_fence;
                while matches!(
                    work_items.first(),
                    Some(WriteWorkItem::Batch {
                        replication_mode: ReplicationCommitMode::Quorum,
                        ..
                    })
                ) {
                    let next_fence_matches = matches!(
                        work_items.first(),
                        Some(WriteWorkItem::Batch { ownership_fence, .. }) if *ownership_fence == merged_fence
                    );
                    if !next_fence_matches {
                        break;
                    }
                    let next = work_items.remove(0);
                    let WriteWorkItem::Batch {
                        ops: next_ops,
                        msg_map: next_map,
                        queue_wait_ns: next_queue_wait_ns,
                        ..
                    } = next
                    else {
                        unreachable!()
                    };
                    let base = merged_ops.len();
                    merged_ops.extend(next_ops);
                    for (msg_idx, start, end) in next_map {
                        merged_map.push((msg_idx, base + start, base + end));
                    }
                    max_queue_wait_ns = max_queue_wait_ns.max(next_queue_wait_ns);
                }
                let preprocessed = preprocess_batch(&merged_ops);
                units.push(ProcessUnit::CoalescedQuorum {
                    ops: merged_ops,
                    msg_map: merged_map,
                    queue_wait_ns: max_queue_wait_ns,
                    preprocessed,
                    ownership_fence: merged_fence,
                });
            }
        }
    }
    units
}

/// Try to process an all-quorum group with pipelined replication: prepare the
/// batch and submit WAL bytes under the engine lock, then spawn replication on a
/// background thread and return immediately. Returns `None` if the group is not
/// eligible for pipelining (mixed replication modes, preparation failure, etc.).
fn try_pipeline_quorum_group(
    _engine: &Arc<RwLock<DbEngine>>,
    _handle: i64,
    _lane_id: usize,
    _shard_ops_accum: &Arc<Mutex<HashMap<u32, u64>>>,
    _group: &[WriteEnvelopeMessage],
    _group_dequeued_at: Instant,
    _mesh: &Arc<PrivateMeshContext>,
) -> Option<InFlightReplication> {
    // Hard-cutover safety: disable the pre-submitted WAL replication pipeline.
    None
}

fn process_write_group_pipelined(
    engine: &Arc<RwLock<DbEngine>>,
    handle: i64,
    lane_id: usize,
    shard_ops_accum: &Arc<std::sync::Mutex<HashMap<u32, u64>>>,
    group: Vec<WriteEnvelopeMessage>,
    group_dequeued_at: Instant,
) -> Option<PendingWalGroup> {
    if group.is_empty() {
        return None;
    }

    // Phase 1: Collect and preprocess batches OUTSIDE the lock.
    let mut work_items: Vec<WriteWorkItem> = Vec::new();
    {
        let mut pending_ops: Vec<BatchOp> = Vec::new();
        let mut pending_map: Vec<(usize, usize, usize)> = Vec::new();
        let mut pending_mode: Option<ReplicationCommitMode> = None;
        let mut pending_fence: Option<OwnershipFence> = None;

        let flush_to_work = |pending_ops: &mut Vec<BatchOp>,
                             pending_map: &mut Vec<(usize, usize, usize)>,
                             pending_mode: &mut Option<ReplicationCommitMode>,
                             pending_fence: &mut Option<OwnershipFence>,
                             work_items: &mut Vec<WriteWorkItem>| {
            if pending_ops.is_empty() {
                return;
            }
            let replication_mode = pending_mode.take().unwrap_or(ReplicationCommitMode::Quorum);
            let ownership_fence = pending_fence.take().unwrap_or_else(|| OwnershipFence {
                expected_home_epoch: 0,
                expected_shard_map_epoch: 0,
                ownership_token: String::new(),
            });
            let now = Instant::now();
            let queue_wait_ns = pending_map
                .iter()
                .filter_map(|(msg_idx, _, _)| group.get(*msg_idx))
                .map(|message| {
                    now.saturating_duration_since(message.enqueued_at)
                        .as_nanos()
                        .min(u64::MAX as u128) as u64
                })
                .max()
                .unwrap_or(0);
            let ops = std::mem::take(pending_ops);
            let preprocessed = preprocess_batch(&ops);
            work_items.push(WriteWorkItem::Batch {
                ops,
                msg_map: std::mem::take(pending_map),
                preprocessed,
                replication_mode,
                ownership_fence,
                queue_wait_ns,
            });
        };

        for (idx, message) in group.iter().enumerate() {
            match &message.envelope {
                WriteEnvelope::Put {
                    namespace,
                    key,
                    value,
                    expected_version,
                    replication_mode,
                    ownership_fence,
                } => {
                    if pending_mode.is_some_and(|mode| mode != *replication_mode)
                        || pending_fence
                            .as_ref()
                            .is_some_and(|fence| fence != ownership_fence)
                    {
                        flush_to_work(
                            &mut pending_ops,
                            &mut pending_map,
                            &mut pending_mode,
                            &mut pending_fence,
                            &mut work_items,
                        );
                    }
                    pending_mode = Some(*replication_mode);
                    pending_fence = Some(ownership_fence.clone());
                    let start = pending_ops.len();
                    pending_ops.push(BatchOp::Put {
                        namespace: namespace.clone(),
                        key: key.clone(),
                        value: value.clone(),
                        expected_version: *expected_version,
                    });
                    pending_map.push((idx, start, start + 1));
                }
                WriteEnvelope::ClientBatch {
                    ops,
                    replication_mode,
                    ownership_fence,
                } => {
                    if pending_mode.is_some_and(|mode| mode != *replication_mode)
                        || pending_fence
                            .as_ref()
                            .is_some_and(|fence| fence != ownership_fence)
                    {
                        flush_to_work(
                            &mut pending_ops,
                            &mut pending_map,
                            &mut pending_mode,
                            &mut pending_fence,
                            &mut work_items,
                        );
                    }
                    pending_mode = Some(*replication_mode);
                    pending_fence = Some(ownership_fence.clone());
                    let start = pending_ops.len();
                    pending_ops.extend_from_slice(ops);
                    pending_map.push((idx, start, pending_ops.len()));
                }
                WriteEnvelope::TxnCommit { txn_id } => {
                    flush_to_work(
                        &mut pending_ops,
                        &mut pending_map,
                        &mut pending_mode,
                        &mut pending_fence,
                        &mut work_items,
                    );
                    work_items.push(WriteWorkItem::TxnCommit {
                        msg_idx: idx,
                        txn_id: *txn_id,
                    });
                }
            }
        }

        flush_to_work(
            &mut pending_ops,
            &mut pending_map,
            &mut pending_mode,
            &mut pending_fence,
            &mut work_items,
        );
    }

    let mut per_message_results: Vec<Result<WriteResult, DbError>> =
        vec![Err(DbError::io("uninitialized write result")); group.len()];

    let wal = match engine.write() {
        Ok(db) => db.wal_for_lane(lane_id),
        Err(_) => {
            for result in &mut per_message_results {
                *result = Err(DbError::io("DB engine lock poisoned"));
            }
            for (msg, result) in group.iter().zip(per_message_results.into_iter()) {
                let _ = msg.response_tx.send(result);
            }
            return None;
        }
    };

    let mut rx_list = Vec::new();
    let mut wal_submitted_at = Vec::new();
    let mut apply_results = Vec::new();
    let mut msg_maps = Vec::new();
    let mut ops_list = Vec::new();
    let mesh = mesh_for_handle(handle).ok().flatten();
    let process_units = coalesce_quorum_batches(work_items);

    // Lock the engine per unit so lanes can interleave under contention.
    for unit in process_units {
        let engine_lock_started = Instant::now();
        let mut db = match engine.write() {
            Ok(db) => db,
            Err(_) => {
                match &unit {
                    ProcessUnit::Single(WriteWorkItem::Batch { msg_map, .. }) => {
                        for (msg_idx, _, _) in msg_map {
                            per_message_results[*msg_idx] =
                                Err(DbError::io("DB engine lock poisoned"));
                        }
                    }
                    ProcessUnit::Single(WriteWorkItem::TxnCommit { msg_idx, .. }) => {
                        per_message_results[*msg_idx] = Err(DbError::io("DB engine lock poisoned"));
                    }
                    ProcessUnit::CoalescedQuorum { msg_map, .. } => {
                        for (msg_idx, _, _) in msg_map {
                            per_message_results[*msg_idx] =
                                Err(DbError::io("DB engine lock poisoned"));
                        }
                    }
                }
                continue;
            }
        };
        let engine_lock_wait_ns = duration_to_nanos(engine_lock_started.elapsed());
        match unit {
            ProcessUnit::Single(WriteWorkItem::Batch {
                ops,
                msg_map,
                preprocessed,
                replication_mode,
                ownership_fence,
                queue_wait_ns,
            }) => {
                let use_outside_lock_replication =
                    matches!(replication_mode, ReplicationCommitMode::Quorum);
                let result = match preprocessed {
                    Ok(pp) => {
                        if use_outside_lock_replication {
                            let prepared = db.prepare_batch_for_outside_replication(
                                &ops,
                                pp,
                                queue_wait_ns,
                                engine_lock_wait_ns,
                                &ownership_fence,
                            );
                            drop(db);
                            match prepared {
                                Ok(prepared) => {
                                    let fanout_result = match mesh.as_deref() {
                                        Some(mesh_ctx) if mesh_ctx.is_leader() => {
                                            replicate_prepared_batch_over_private_mesh(
                                                mesh_ctx, &prepared,
                                            )
                                        }
                                        _ => replicate_prepared_batch_with_local_simulation(
                                            &prepared,
                                        ),
                                    };
                                    let mut db = match engine.write() {
                                        Ok(db) => db,
                                        Err(_) => {
                                            for (msg_idx, _, _) in &msg_map {
                                                per_message_results[*msg_idx] =
                                                    Err(DbError::io("DB engine lock poisoned"));
                                            }
                                            continue;
                                        }
                                    };
                                    db.finalize_prepared_batch_after_outside_replication(
                                        prepared,
                                        fanout_result,
                                    )
                                }
                                Err(err) => Err(err),
                            }
                        } else {
                            db.prepare_and_apply_batch(
                                &ops,
                                pp,
                                queue_wait_ns,
                                engine_lock_wait_ns,
                                replication_mode,
                                mesh.as_deref(),
                                &ownership_fence,
                            )
                        }
                    }
                    Err(err) => Err(err),
                };
                match result {
                    Ok(mut apply_result) => {
                        let wal_ops = apply_result.wal_ops;
                        let rx = if apply_result.wal_bytes.is_empty()
                            && !apply_result.wal_records.is_empty()
                        {
                            let (encode_ns, rx) = encode_records_to_wal_bytes(
                                &apply_result.wal_records,
                                |bytes, encode_ns| {
                                    (
                                        encode_ns,
                                        wal.append_raw_bytes_submit_slice(
                                            bytes, wal_ops, encode_ns,
                                        ),
                                    )
                                },
                            );
                            apply_result.encode_ns = encode_ns;
                            apply_result.wal_records.clear();
                            rx
                        } else {
                            wal.append_raw_bytes_submit(
                                std::mem::take(&mut apply_result.wal_bytes),
                                apply_result.wal_ops,
                                apply_result.encode_ns,
                            )
                        };
                        let rx = rx.unwrap_or_else(|err| {
                            panic!(
                                "FATAL: local WAL submit failed after Raft quorum; IO Error: {}",
                                err
                            )
                        });
                        if let Some((shard_id, op_count)) = apply_result.shard_ops_delta {
                            if let Ok(mut accum) = shard_ops_accum.lock() {
                                let c = accum.entry(shard_id).or_insert(0);
                                *c = c.saturating_add(op_count);
                            }
                        }
                        rx_list.push(rx);
                        wal_submitted_at.push(Instant::now());
                        apply_results.push(apply_result);
                        msg_maps.push(msg_map);
                        ops_list.push(ops);
                    }
                    Err(err) => {
                        for (msg_idx, _, _) in &msg_map {
                            per_message_results[*msg_idx] = Err(err.clone());
                        }
                    }
                }
            }
            ProcessUnit::Single(WriteWorkItem::TxnCommit { msg_idx, txn_id }) => {
                per_message_results[msg_idx] =
                    db.txn_commit(txn_id).map(|_| WriteResult::TxnCommitted);
            }
            ProcessUnit::CoalescedQuorum {
                ops,
                msg_map,
                queue_wait_ns,
                preprocessed,
                ownership_fence,
            } => {
                let result = match preprocessed {
                    Ok(pp) => {
                        let prepared = db.prepare_batch_for_outside_replication(
                            &ops,
                            pp,
                            queue_wait_ns,
                            engine_lock_wait_ns,
                            &ownership_fence,
                        );
                        drop(db);
                        match prepared {
                            Ok(prepared) => {
                                let fanout_result = match mesh.as_deref() {
                                    Some(mesh_ctx) if mesh_ctx.is_leader() => {
                                        replicate_prepared_batch_over_private_mesh(
                                            mesh_ctx, &prepared,
                                        )
                                    }
                                    _ => replicate_prepared_batch_with_local_simulation(&prepared),
                                };
                                let mut db = match engine.write() {
                                    Ok(db) => db,
                                    Err(_) => {
                                        for (msg_idx, _, _) in &msg_map {
                                            per_message_results[*msg_idx] =
                                                Err(DbError::io("DB engine lock poisoned"));
                                        }
                                        continue;
                                    }
                                };
                                db.finalize_prepared_batch_after_outside_replication(
                                    prepared,
                                    fanout_result,
                                )
                            }
                            Err(err) => Err(err),
                        }
                    }
                    Err(err) => Err(err),
                };
                match result {
                    Ok(mut apply_result) => {
                        let wal_ops = apply_result.wal_ops;
                        let rx = if apply_result.wal_bytes.is_empty()
                            && !apply_result.wal_records.is_empty()
                        {
                            let (encode_ns, rx) = encode_records_to_wal_bytes(
                                &apply_result.wal_records,
                                |bytes, encode_ns| {
                                    (
                                        encode_ns,
                                        wal.append_raw_bytes_submit_slice(
                                            bytes, wal_ops, encode_ns,
                                        ),
                                    )
                                },
                            );
                            apply_result.encode_ns = encode_ns;
                            apply_result.wal_records.clear();
                            rx
                        } else {
                            wal.append_raw_bytes_submit(
                                std::mem::take(&mut apply_result.wal_bytes),
                                apply_result.wal_ops,
                                apply_result.encode_ns,
                            )
                        };
                        let rx = rx.unwrap_or_else(|err| {
                            panic!(
                                "FATAL: local WAL submit failed after Raft quorum; IO Error: {}",
                                err
                            )
                        });
                        if let Some((shard_id, op_count)) = apply_result.shard_ops_delta {
                            if let Ok(mut accum) = shard_ops_accum.lock() {
                                let c = accum.entry(shard_id).or_insert(0);
                                *c = c.saturating_add(op_count);
                            }
                        }
                        rx_list.push(rx);
                        wal_submitted_at.push(Instant::now());
                        apply_results.push(apply_result);
                        msg_maps.push(msg_map);
                        ops_list.push(ops);
                    }
                    Err(err) => {
                        for (msg_idx, _, _) in &msg_map {
                            per_message_results[*msg_idx] = Err(err.clone());
                        }
                    }
                }
            }
        }
    }

    Some(PendingWalGroup {
        group_dequeued_at,
        completion_results: (0..rx_list.len()).map(|_| None).collect(),
        rx_list,
        wal_submitted_at,
        apply_results,
        msg_maps,
        ops_list,
        group,
        per_message_results,
    })
}

fn finish_pending_wal_group(
    engine: &Arc<RwLock<DbEngine>>,
    handle: i64,
    pending: PendingWalGroup,
    completions: Vec<io::Result<WalBatchCompletion>>,
) {
    let mut deferred_persists = Vec::new();
    let mut apply_tasks = Vec::new();
    let mut per_message_results = pending.per_message_results;

    let apply_results = pending.apply_results;
    let msg_maps = pending.msg_maps;
    let ops_list = pending.ops_list;
    let wal_submitted_at = pending.wal_submitted_at;

    for (((completion, mut apply_result), (msg_map, _ops)), submitted_at) in completions
        .into_iter()
        .zip(apply_results)
        .zip(msg_maps.into_iter().zip(ops_list))
        .zip(wal_submitted_at)
    {
        let completed_at = Instant::now();
        let completion = match completion {
            Ok(c) => c,
            Err(err) => {
                for result in &mut per_message_results {
                    *result = Err(DbError::io(err.to_string()));
                }
                break;
            }
        };
        let wal_metrics = completion.metrics;
        let wal_submit_wait_ns = duration_to_nanos(
            completion
                .completed_at
                .saturating_duration_since(submitted_at),
        );
        let wal_hol_wait_ns =
            duration_to_nanos(completed_at.saturating_duration_since(completion.completed_at));
        let lane_dequeue_to_complete_ns =
            duration_to_nanos(completed_at.saturating_duration_since(pending.group_dequeued_at));
        let queue_to_complete_ns = apply_result
            .stage_data
            .queue_wait_ns
            .saturating_add(lane_dequeue_to_complete_ns);

        let wal_append_ns = apply_result
            .encode_ns
            .saturating_add(wal_submit_wait_ns)
            .saturating_add(wal_hol_wait_ns);
        let sample = DbWriteStageSample {
            op_count: apply_result.stage_data.op_count,
            byte_count: apply_result.stage_data.byte_count,
            queue_wait_ns: apply_result.stage_data.queue_wait_ns,
            engine_lock_wait_ns: apply_result.stage_data.engine_lock_wait_ns,
            validate_route_ns: apply_result.stage_data.validate_route_ns,
            replicate_ns: apply_result.stage_data.replicate_ns,
            wal_append_ns,
            wal_submit_wait_ns,
            wal_hol_wait_ns,
            wal_queue_wait_ns: wal_metrics.queue_wait_ns,
            wal_encode_ns: wal_metrics.encode_ns,
            wal_fdatasync_ns: wal_metrics.fdatasync_ns,
            wal_mutex_wait_ns: wal_metrics.mutex_wait_ns,
            apply_ns: apply_result.stage_data.apply_ns,
            raft_persist_ns: apply_result.stage_data.raft_persist_ns,
            clock_persist_ns: apply_result.stage_data.clock_persist_ns,
            total_ns: duration_to_nanos(apply_result.stage_data.total_started.elapsed()),
            lane_dequeue_to_complete_ns,
            queue_to_complete_ns,
        };
        if let Ok(mut db) = engine.write() {
            db.write_stage.record(sample);
            db.mark_group_durable(apply_result.active_group_id, apply_result.required_index);
            if apply_result.staged_ops.is_empty() {
                db.mark_group_apply_visible(
                    apply_result.active_group_id,
                    apply_result.required_index,
                );
            } else {
                apply_tasks.push(ApplyTask {
                    active_group_id: apply_result.active_group_id,
                    required_index: apply_result.required_index,
                    staged_ops: std::mem::take(&mut apply_result.staged_ops),
                });
            }
        }
        deferred_persists.push(apply_result.deferred);
        for (msg_idx, start, end) in msg_map {
            let max_version = apply_result.committed_versions[start..end]
                .iter()
                .copied()
                .max()
                .unwrap_or(0);
            per_message_results[msg_idx] = Ok(WriteResult::Version(max_version));
        }
    }

    if !apply_tasks.is_empty() {
        if let Ok(pool) = apply_lane_pool_for_handle(handle) {
            for task in apply_tasks {
                if let Err(task) = pool.enqueue(task)
                    && let Ok(mut db) = engine.write()
                {
                    db.apply_committed_task(task);
                }
            }
        } else if let Ok(mut db) = engine.write() {
            for task in apply_tasks {
                db.apply_committed_task(task);
            }
        }
    }

    for (msg, result) in pending.group.iter().zip(per_message_results.into_iter()) {
        let _ = msg.response_tx.send(result);
    }

    for deferred in deferred_persists {
        deferred.execute();
    }
}

impl DbEngine {
    pub fn open(path: &Path) -> Result<Self, DbError> {
        #[cfg(test)]
        {
            return Self::open_with_config(path, &DbConfig::for_testing());
        }
        #[cfg(not(test))]
        {
            let config = DbConfig::from_env_strict().map_err(|err| {
                DbError::invalid_argument(format!("STRICT_CONFIG_INVALID: {err}"))
            })?;
            Self::open_with_config(path, &config)
        }
    }

    pub fn open_with_config(path: &Path, config: &DbConfig) -> Result<Self, DbError> {
        if let Some(message) = strict_replication_validation_message(
            config.replication.factor,
            config.replication.write_quorum,
        ) {
            return Err(DbError::invalid_argument(format!(
                "STRICT_CONFIG_INVALID: {message}"
            )));
        }
        let replication_factor = config.replication.factor;
        let write_quorum = config.replication.write_quorum;
        let local_region = normalize_region_id(&config.topology.local_region).ok_or_else(|| {
            DbError::invalid_argument(
                "SOVEREIGNTY_REGION_UNRESOLVED: topology.local_region must be non-empty",
            )
        })?;
        let topology_region_az_node_map =
            canonicalize_topology_region_az_node_map(&config.topology.region_az_node_map);
        let topology_canonical_regions =
            canonical_region_set_from_map(&topology_region_az_node_map);
        let checkpoint_allowed_regions = normalize_region_list(&config.checkpoint.allowed_regions);
        let sovereignty_allowed_regions =
            normalize_region_list(&config.sovereignty.allowed_regions);
        let checkpoint_config = resolve_checkpoint_config(
            path.parent().unwrap_or_else(|| Path::new(".")),
            &config.checkpoint,
        );
        let checkpoint_manager = match checkpoint_config.build_manager() {
            Ok(manager) => Some(manager),
            Err(err) => {
                if config.restore_latest_checkpoint_on_open {
                    return Err(DbError::io(format!(
                        "CHECKPOINT_RESTORE_ON_OPEN_FAILED: checkpoint manager init failed: {err}"
                    )));
                }
                None
            }
        };
        if config.restore_latest_checkpoint_on_open {
            if let Some(data_dir) = path.parent() {
                if path.exists() {
                    // WAL exists locally; prefer local fast path and only restore when open fails.
                } else if should_restore_latest_checkpoint_on_open(&checkpoint_config) {
                    let manager = checkpoint_manager.as_ref().ok_or_else(|| {
                        DbError::io(
                            "CHECKPOINT_RESTORE_ON_OPEN_FAILED: checkpoint manager unavailable",
                        )
                    })?;
                    manager.restore_latest(data_dir).map_err(|err| {
                        DbError::io(format!("CHECKPOINT_RESTORE_ON_OPEN_FAILED: {err}"))
                    })?;
                }
            }
        }
        let wal = Arc::new(WalSegment::open(path).map_err(|err| DbError::io(err.to_string()))?);
        let mut records = recover(wal.as_ref()).map_err(|err| DbError::io(err.to_string()))?;
        let data_dir = path.parent().unwrap_or_else(|| Path::new("."));
        let lane_count = config.engine.writer_lane_count.max(1);
        let mut lane_wals: Vec<Arc<WalSegment>> = Vec::with_capacity(lane_count);
        for lane_id in 0..lane_count {
            let lane_path = wal_lane_path_from(data_dir, lane_id);
            let lane_wal =
                Arc::new(WalSegment::open(&lane_path).map_err(|err| DbError::io(err.to_string()))?);
            let mut lane_records =
                recover(lane_wal.as_ref()).map_err(|err| DbError::io(err.to_string()))?;
            records.append(&mut lane_records);
            lane_wals.push(lane_wal);
        }
        records.sort_by_key(|r| r.version);
        let cdc_checkpoints = load_cdc_checkpoints(path)?;
        let clock = HybridLogicalClock::new();
        let uncertainty = UncertaintyTracker::new(DEFAULT_MAX_CLOCK_SKEW_MS);
        let watermarks = SafeReadWatermarks::new();
        let mut safe_time = SafeTimePropagator::default();
        let persisted_primary_raft =
            load_persisted_raft_state(path).map_err(|err| DbError::io(err.to_string()))?;
        let binary_raft_meta =
            load_raft_metadata_binary(path).map_err(|err| DbError::io(err.to_string()))?;
        let mut wal_raft_meta: Option<RaftPersistMetadata> = None;
        let mut leader = NodeState::with_timing(LOCAL_NODE_ID, 0, 10);
        leader.current_term = 1;
        let mut membership = default_membership_for_replication_factor(replication_factor)?;
        if let Some(persisted) = persisted_primary_raft.as_ref() {
            membership = persisted
                .restore(&mut leader, 0, 10)
                .map_err(|err| DbError::io(err.to_string()))?;
        }
        for rec in &records {
            if let RecordKind::RaftMeta = rec.kind {
                if let Some((current_term, voted_for, commit_index, _)) =
                    decode_raft_meta_value(&rec.value)
                {
                    wal_raft_meta = Some(RaftPersistMetadata {
                        current_term,
                        voted_for,
                        commit_index,
                        needs_membership_flush: false,
                    });
                }
            }
        }
        let raft_meta = if matches!(
            config.replication.log_backend,
            ReplicatedLogBackend::CanonicalOnly
        ) {
            binary_raft_meta.or(wal_raft_meta)
        } else {
            wal_raft_meta.or(binary_raft_meta)
        };
        if let Some(meta) = raft_meta.as_ref() {
            if meta.current_term >= leader.current_term {
                leader.current_term = meta.current_term;
                leader.voted_for = meta.voted_for;
                leader.commit_index = meta.commit_index.min(leader.last_log_index());
            }
        }
        let legacy_replication_template = ReplicationState {
            leader: leader.clone(),
            followers: HashMap::new(),
            membership: membership.clone(),
            durability_commit_index: leader.commit_index,
            apply_visible_index: leader.commit_index,
        };

        if let Some(persisted) = load_hlc_state(path).map_err(|err| DbError::io(err.to_string()))? {
            clock.observe_packed(persisted);
            uncertainty.observe_remote_packed(persisted);
            watermarks.observe(LOCAL_NODE_ID, persisted);
            safe_time.observe_shard_safe_time("clock", LOCAL_REGION_ID, persisted);
        }

        let persisted_topology =
            load_persisted_topology_state(path).map_err(|err| DbError::io(err.to_string()))?;
        let (
            shard_directory,
            mut replication_groups,
            persisted_autoscale_status,
            loaded_rf,
            loaded_wq,
        ) = if let Some(topology) = persisted_topology {
            let shard_directory = ShardDirectory::from_snapshot(topology.shard_directory.clone())
                .map_err(|err| {
                DbError::io(format!(
                    "invalid persisted topology shard directory: {err:?}"
                ))
            })?;
            let mut replication_groups = HashMap::new();
            for group in topology.groups {
                let mut restored_leader = NodeState::with_timing(LOCAL_NODE_ID, 0, 10);
                let restored_membership = group
                    .raft
                    .restore(&mut restored_leader, 0, 10)
                    .map_err(|err| DbError::io(err.to_string()))?;
                replication_groups.insert(
                    group.group_id,
                    ReplicationState {
                        durability_commit_index: restored_leader.commit_index,
                        apply_visible_index: restored_leader.commit_index,
                        leader: restored_leader,
                        followers: HashMap::new(),
                        membership: restored_membership,
                    },
                );
            }
            if let Some(persisted_primary) = persisted_primary_raft.as_ref() {
                let mut restored_leader = NodeState::with_timing(LOCAL_NODE_ID, 0, 10);
                let restored_membership = persisted_primary
                    .restore(&mut restored_leader, 0, 10)
                    .map_err(|err| DbError::io(err.to_string()))?;
                let persisted_index = restored_leader.last_log_index();
                let topology_index = replication_groups
                    .get(&PRIMARY_ACTIVE_GROUP_ID)
                    .map(|replication| replication.leader.last_log_index())
                    .unwrap_or(0);
                if persisted_index >= topology_index {
                    replication_groups.insert(
                        PRIMARY_ACTIVE_GROUP_ID,
                        ReplicationState {
                            durability_commit_index: restored_leader.commit_index,
                            apply_visible_index: restored_leader.commit_index,
                            leader: restored_leader,
                            followers: HashMap::new(),
                            membership: restored_membership,
                        },
                    );
                }
            }
            (
                shard_directory,
                replication_groups,
                topology.autoscale_status,
                topology.replication_factor,
                topology.write_quorum,
            )
        } else {
            let shard_directory = ShardDirectory::new(
                config.topology.initial_logical_shards,
                config.topology.initial_active_groups,
            )
            .map_err(|err| {
                DbError::invalid_argument(format!("invalid shard directory config: {err:?}"))
            })?;
            let mut replication_groups = HashMap::new();
            for group_id in 0..shard_directory.active_group_count() {
                replication_groups.insert(group_id, legacy_replication_template.clone());
            }
            (
                shard_directory,
                replication_groups,
                None,
                replication_factor,
                write_quorum,
            )
        };
        if let Some(message) = strict_replication_validation_message(loaded_rf, loaded_wq) {
            return Err(DbError::invalid_argument(format!(
                "STRICT_CONFIG_INVALID: persisted topology {message}"
            )));
        }
        let mut loaded_rf = loaded_rf;
        let mut loaded_wq = loaded_wq;
        for group_id in 0..shard_directory.active_group_count() {
            replication_groups
                .entry(group_id)
                .or_insert_with(|| legacy_replication_template.clone());
        }
        let observed_voters = replication_groups
            .get(&PRIMARY_ACTIVE_GROUP_ID)
            .map(|replication| replication.membership.voters().len() as u32)
            .unwrap_or(loaded_rf)
            .max(1);
        loaded_rf = loaded_rf.max(observed_voters);
        loaded_wq = loaded_wq.min(observed_voters).max(1);
        if let Some(message) = strict_replication_validation_message(loaded_rf, loaded_wq) {
            return Err(DbError::invalid_argument(format!(
                "STRICT_CONFIG_INVALID: persisted topology {message}"
            )));
        }

        let mut memtable = Memtable::default();
        let mut blob_store = BlobStore::default();
        let mut replay_blob_values_externalized = 0u64;
        for rec in records {
            let user_key = encode_user_key(&rec.namespace, &rec.key)?;
            let value = match rec.kind {
                RecordKind::Put => {
                    let (stored, externalized) =
                        externalize_value_for_memtable(&mut blob_store, rec.value);
                    if externalized {
                        replay_blob_values_externalized =
                            replay_blob_values_externalized.saturating_add(1);
                    }
                    Some(stored)
                }
                RecordKind::Delete => None,
                RecordKind::RaftMeta | RecordKind::Unknown(_) => continue,
            };
            memtable.apply_owned(user_key, rec.version, value);
            clock.observe_packed(rec.version);
            uncertainty.observe_remote_packed(rec.version);
            watermarks.observe(LOCAL_NODE_ID, rec.version);
            safe_time.observe_shard_safe_time(
                String::from_utf8_lossy(&rec.namespace).to_string(),
                LOCAL_REGION_ID,
                rec.version,
            );
        }
        let current_clock = clock.peek().pack();
        watermarks.observe(LOCAL_NODE_ID, current_clock);
        safe_time.observe_shard_safe_time("clock", LOCAL_REGION_ID, current_clock);
        persist_hlc_state(path, current_clock).map_err(|err| DbError::io(err.to_string()))?;
        let autoscale_status = if let Some(status) = persisted_autoscale_status {
            DbAutoscaleStatus {
                enabled: config.topology.autoscale_enabled,
                mode: AutoscaleMode::GrowOnly,
                last_action: status.last_action,
                reasons: status.reasons,
                cooldown_ms: config.topology.autoscale_tick_ms,
                last_action_at_epoch_ms: status.last_action_at_epoch_ms,
            }
        } else {
            DbAutoscaleStatus {
                enabled: config.topology.autoscale_enabled,
                mode: AutoscaleMode::GrowOnly,
                last_action: "initialized".to_string(),
                reasons: vec!["autoscale ready".to_string()],
                cooldown_ms: config.topology.autoscale_tick_ms,
                last_action_at_epoch_ms: 0,
            }
        };

        let initial_lsm_stats = memtable.stats();
        let autopilot_bootstrap = crate::db::autopilot::orchestrator::execute_controller_tick(
            crate::db::autopilot::orchestrator::ControllerInput {
                action_id: 0,
                source: "boot-init".to_string(),
                now_epoch_ms: now_epoch_ms(),
                replication_factor: loaded_rf,
                write_quorum: loaded_wq,
                autoscale_enabled: config.topology.autoscale_enabled,
                active_groups: shard_directory.active_group_count(),
                logical_shards: shard_directory.logical_shard_count(),
                observed_live_bytes: initial_lsm_stats.live_bytes_estimate,
                hot_meta_write_ops: 0,
                hot_meta_max_write_ops: AUTOPILOT_HOTMETA_MAX_WRITE_OPS_PER_TICK,
                tiering_boundary_min_live_bytes: AUTOPILOT_TIERING_MIN_LIVE_BYTES,
                tiering_boundary_max_live_bytes: AUTOPILOT_TIERING_MAX_LIVE_BYTES,
            },
        );
        let mut autopilot_audit_ring = crate::db::autopilot::orchestrator::AuditRingBuffer::new(
            crate::db::autopilot::orchestrator::DEFAULT_AUDIT_RING_CAPACITY,
        );
        autopilot_audit_ring.push(autopilot_bootstrap.audit_row.clone());

        let mut engine = Self {
            memtable,
            blob_store,
            read_path: ReadPath::new(
                DEFAULT_POINT_READ_IN_FLIGHT_LIMIT,
                DEFAULT_RANGE_READ_IN_FLIGHT_LIMIT,
                DEFAULT_POINT_READ_CACHE_CAPACITY,
                DEFAULT_NEGATIVE_BLOOM_CAPACITY,
            ),
            wal,
            lane_wals,
            replication_groups,
            raft_current_term: 1,
            raft_last_log_index: 0,
            raft_last_committed_index: 0,
            raft_persist_interval_ops: RAFT_PERSIST_INTERVAL_OPS,
            raft_persist_ops_since_flush: 0,
            clock_persist_interval_ops: CLOCK_PERSIST_INTERVAL_OPS,
            clock_persist_ops_since_flush: 0,
            #[cfg(test)]
            pending_append_responses: Vec::new(),
            clock,
            next_txn_id: 1,
            txns: HashMap::new(),
            lock_table: TxnLockTable::default(),
            cdc: CdcEmitter::default(),
            cdc_checkpoints,
            next_snapshot_id: 1,
            snapshots: HashMap::new(),
            wal_path: path.to_path_buf(),
            uncertainty,
            watermarks,
            safe_time,
            clock_persist_error: None,
            clock_persist_error_at: None,
            raft_persist_error: None,
            raft_persist_error_at: None,
            cdc_checkpoint_persist_error: None,
            cdc_checkpoint_persist_error_at: None,
            checkpoint_persist_error: None,
            checkpoint_persist_error_at: None,
            checkpoint_restore_error: None,
            checkpoint_restore_error_at: None,
            schema_gate_error: None,
            schema_gate_error_at: None,
            topology_state_dirty: true,
            schema_committed_epoch: 1,
            schema_all_voters_on_target_binary: true,
            checkpoint_interval_secs: config.checkpoint.interval_secs.max(1),
            checkpoint_last_epoch_s: now_epoch_s(),
            checkpoint_config,
            checkpoint_manager,
            local_region,
            topology_region_az_node_map,
            topology_canonical_regions,
            checkpoint_allowed_regions,
            sovereignty_id: config.sovereignty.id.clone(),
            sovereignty_allowed_regions,
            sovereignty_enforce_all_copies: config.sovereignty.enforce_all_copies,
            replication_async_failover: config.replication.async_failover,
            residency_policy: config.residency_policy.clone(),
            shard_directory,
            home_store: crate::db::placement::PlacementHomeStore::default(),
            keyrange_ownership: BTreeMap::new(),
            replication_factor: loaded_rf,
            write_quorum: loaded_wq,
            quorum_transport_mode: config.replication.quorum_transport_mode,
            commit_visibility_mode: config.replication.commit_visibility_mode,
            replicated_log_backend: config.replication.log_backend,
            autoscale_enabled: config.topology.autoscale_enabled,
            autoscale_mode: AutoscaleMode::GrowOnly,
            autoscale_tick_ms: config.topology.autoscale_tick_ms,
            autoscale_max_skew_ratio: config.topology.autoscale_max_skew_ratio,
            autoscale_target_shards_per_group: config.topology.autoscale_target_shards_per_group,
            autoscale_max_active_groups: config.topology.autoscale_max_active_groups,
            autoscale_max_logical_shards: config.topology.autoscale_max_logical_shards,
            autoscale_last_tick_epoch_ms: 0,
            autoscale_status,
            intent_config: config.intent.clone(),
            autopilot_action_seq: 0,
            autopilot_intent_effective: autopilot_bootstrap.intent_effective,
            autopilot_intent_conflicts: autopilot_bootstrap.intent_conflicts,
            autopilot_tiering_state: autopilot_bootstrap.tiering_state,
            autopilot_recommendations: autopilot_bootstrap.recommendations,
            autopilot_audit_ring,
            shard_write_ops: HashMap::new(),
            shard_write_ops_accum: Arc::new(std::sync::Mutex::new(HashMap::new())),
            write_stage: WriteStageTelemetry::default(),
            client_write_path: ClientWritePathTelemetry::default(),
            replication_telemetry: ReplicationTelemetry::default(),
            replicated_log_telemetry: ReplicatedLogTelemetry::default(),
            jupiter_telemetry: JupiterFeatureTelemetry {
                blob_values_externalized: replay_blob_values_externalized,
                ..JupiterFeatureTelemetry::default()
            },
            sorted_run_installs: BTreeMap::new(),
            sorted_run_progress: HashMap::new(),
            apply_backlog_peak: 0,
            lsm_cached_stats: initial_lsm_stats,
            lsm_stats_dirty: false,
            lsm_stats_ops_since_refresh: 0,
            lsm_stats_refresh_ops_interval: LSM_STATS_REFRESH_OPS_INTERVAL,
            blob_gc_ops_since_run: 0,
            blob_gc_ops_interval: BLOB_GC_OPS_INTERVAL,
            memtable_gc_write_counter: 0,
            #[cfg(test)]
            autoscale_test_healthy_nodes: None,
            #[cfg(test)]
            fail_next_cdc_checkpoint_persist: false,
        };
        let primary = engine.primary_replication().clone();
        engine.raft_current_term = primary.leader.current_term.max(1);
        engine.raft_last_log_index = primary.leader.last_log_index();
        engine.raft_last_committed_index = primary.leader.commit_index;
        engine.refresh_replication_followers();
        engine.sync_keyrange_ownership_state()?;
        engine.persist_topology_state_best_effort();
        Ok(engine)
    }

    fn persist_clock_state(&self, packed: u64) -> Result<(), DbError> {
        persist_hlc_state(&self.wal_path, packed).map_err(|err| DbError::io(err.to_string()))
    }

    fn tick_clock(&mut self) -> u64 {
        let packed = self.clock.tick().pack();
        self.watermarks.observe(LOCAL_NODE_ID, packed);
        packed
    }

    /// Batch HLC ticks: one wall-clock read for the whole batch, then observe each to watermarks.
    fn tick_batch_clock(&mut self, count: usize) -> Vec<u64> {
        let packed = self.clock.tick_batch(count);
        for &p in &packed {
            self.watermarks.observe(LOCAL_NODE_ID, p);
        }
        packed
    }

    fn primary_replication(&self) -> &ReplicationState {
        self.replication_groups
            .get(&PRIMARY_ACTIVE_GROUP_ID)
            .expect("BUG: primary replication group missing — open_db must create it")
    }

    #[cfg(test)]
    fn primary_replication_mut(&mut self) -> &mut ReplicationState {
        self.replication_groups
            .get_mut(&PRIMARY_ACTIVE_GROUP_ID)
            .expect("primary replication group must exist")
    }

    fn replication_for_group(&self, active_group_id: u32) -> Result<&ReplicationState, DbError> {
        self.replication_groups
            .get(&active_group_id)
            .ok_or_else(|| {
                DbError::invalid_argument(format!("unknown active group id {active_group_id}"))
            })
    }

    fn replication_for_group_mut(
        &mut self,
        active_group_id: u32,
    ) -> Result<&mut ReplicationState, DbError> {
        self.replication_groups
            .get_mut(&active_group_id)
            .ok_or_else(|| {
                DbError::invalid_argument(format!("unknown active group id {active_group_id}"))
            })
    }

    #[allow(dead_code)]
    fn persist_clock_state_best_effort(&mut self, packed: u64) {
        if let Err(err) = self.persist_clock_state(packed) {
            self.clock_persist_error = Some(err.message);
            self.clock_persist_error_at = Some(now_epoch_s());
        } else {
            self.clock_persist_error = None;
            self.clock_persist_error_at = None;
        }
    }

    #[allow(dead_code)]
    fn maybe_persist_clock_state_best_effort(&mut self, packed: u64) {
        if self.clock_persist_interval_ops <= 1 {
            self.persist_clock_state_best_effort(packed);
            self.clock_persist_ops_since_flush = 0;
            return;
        }
        self.clock_persist_ops_since_flush = self.clock_persist_ops_since_flush.saturating_add(1);
        if self.clock_persist_ops_since_flush >= self.clock_persist_interval_ops {
            self.persist_clock_state_best_effort(packed);
            self.clock_persist_ops_since_flush = 0;
        }
    }

    fn write_stage_aggregate(&self, queue: WriteLaneTelemetrySnapshot) -> DbWriteStageAggregate {
        self.write_stage.snapshot(queue)
    }

    fn client_write_path_aggregate(&self) -> DbClientWritePathAggregate {
        self.client_write_path.snapshot()
    }

    fn record_client_write_path_sample(&mut self, sample: DbClientWritePathSample) {
        self.client_write_path.record(sample);
    }

    fn record_insert_fast_lane_attempt(&mut self, accepted: bool) {
        self.jupiter_telemetry
            .record_insert_fast_lane_attempt(accepted);
    }

    fn run_blob_gc_cycle(&mut self) {
        let referenced_blob_ids = self
            .memtable
            .referenced_blob_ids(|value| decode_blob_ref_value(value).map(|(blob_id, _)| blob_id));
        let _reclaimed = self.blob_store.gc_unreferenced(&referenced_blob_ids);
        let metrics = self.blob_store.metrics();
        self.jupiter_telemetry.blob_gc_runs = metrics.gc_runs;
        self.jupiter_telemetry.blob_gc_reclaimed_bytes = metrics.reclaimed_bytes;
    }

    fn maybe_run_blob_gc_cycle(&mut self) {
        if !blob_gc_active() {
            return;
        }
        self.blob_gc_ops_since_run = self.blob_gc_ops_since_run.saturating_add(1);
        if self.blob_gc_ops_since_run >= self.blob_gc_ops_interval.max(1) {
            self.blob_gc_ops_since_run = 0;
            self.run_blob_gc_cycle();
        }
    }

    fn mark_lsm_stats_dirty(&mut self) {
        self.lsm_stats_dirty = true;
    }

    fn refresh_lsm_stats_cache(&mut self) {
        self.lsm_cached_stats = self.memtable.stats();
        self.lsm_stats_dirty = false;
        self.lsm_stats_ops_since_refresh = 0;
    }

    fn lsm_stats_for_scheduler(&mut self) -> MemtableStats {
        if !self.lsm_stats_dirty {
            return self.lsm_cached_stats;
        }
        self.lsm_stats_ops_since_refresh = self.lsm_stats_ops_since_refresh.saturating_add(1);
        if self.lsm_stats_ops_since_refresh >= self.lsm_stats_refresh_ops_interval.max(1) {
            self.refresh_lsm_stats_cache();
        }
        self.lsm_cached_stats
    }

    fn lsm_stats_for_health_snapshot(&mut self) -> MemtableStats {
        if self.lsm_stats_dirty {
            self.refresh_lsm_stats_cache();
        }
        self.lsm_cached_stats
    }

    fn current_apply_backlog_depth_for_group(&self, active_group_id: u32) -> u64 {
        self.replication_for_group(active_group_id)
            .map(|replication| {
                replication
                    .durability_commit_index
                    .saturating_sub(replication.apply_visible_index)
            })
            .unwrap_or(0)
    }

    fn compaction_admission_decision_for_group(
        &mut self,
        active_group_id: u32,
    ) -> crate::db::lsm::scheduler::CompactionAdmissionDecision {
        let debt_bytes = self
            .lsm_stats_for_scheduler()
            .compaction_debt_bytes_estimate;
        let in_flight_jobs = self.current_apply_backlog_depth_for_group(active_group_id) as usize;
        crate::db::lsm::scheduler::decide_compaction_admission(
            debt_bytes,
            in_flight_jobs,
            crate::db::lsm::scheduler::CompactionSchedulerConfig {
                max_debt_bytes: compaction_scheduler_max_debt_bytes_default(),
                max_in_flight_jobs: 1,
            },
        )
    }

    fn apply_sorted_run_chunk_payload(&mut self, payload: &[u8]) -> Result<(), String> {
        let entries = crate::db::lsm::sstable::decode_block(payload)
            .map_err(|err| format!("SORTED_RUN_CHUNK_INVALID_ENCODING: {err:?}"))?;
        let mut max_applied_version = None;
        for entry in entries {
            let value = match (entry.value, entry.value_blob_ref) {
                (Some(value), None) => {
                    let (stored, externalized) =
                        externalize_value_for_memtable(&mut self.blob_store, Bytes::from(value));
                    if externalized {
                        self.jupiter_telemetry.blob_values_externalized = self
                            .jupiter_telemetry
                            .blob_values_externalized
                            .saturating_add(1);
                    }
                    Some(stored)
                }
                (None, None) => None,
                (None, Some(_)) => {
                    return Err("SORTED_RUN_CHUNK_BLOB_REF_UNSUPPORTED".to_string());
                }
                (Some(_), Some(_)) => {
                    return Err("SORTED_RUN_CHUNK_INVALID_VALUE_STATE".to_string());
                }
            };
            self.memtable.apply(&entry.key, entry.version, value);
            self.clock.observe_packed(entry.version);
            self.uncertainty.observe_remote_packed(entry.version);
            self.watermarks.observe(LOCAL_NODE_ID, entry.version);
            let namespace = decode_user_key(&entry.key)
                .ok()
                .map(|(ns, _)| String::from_utf8_lossy(&ns).into_owned())
                .unwrap_or_else(|| "catchup".to_string());
            self.safe_time.observe_shard_safe_time_no_recompute(
                namespace,
                LOCAL_REGION_ID,
                entry.version,
            );
            max_applied_version = Some(max_applied_version.unwrap_or(0).max(entry.version));
        }
        if let Some(version) = max_applied_version {
            self.safe_time
                .observe_shard_safe_time_no_recompute("clock", LOCAL_REGION_ID, version);
            self.safe_time.recompute_region_safe_times();
        }
        self.mark_lsm_stats_dirty();
        Ok(())
    }

    /// Apply pre-encoded WAL bytes directly to the memtable. Used by the follower
    /// fast-path (Opt 2) to skip the writer-lane queue and WAL re-encoding.
    /// Decodes WAL records from `wal_bytes` and applies Put/Delete entries to the
    /// memtable, clock, uncertainty, watermarks, and safe_time — mirroring the
    /// pattern in `apply_sorted_run_chunk_payload`.
    fn apply_wal_records_direct(&mut self, wal_bytes: &[u8]) -> Result<u64, DbError> {
        let mut max_applied_version: Option<u64> = None;
        let mut offset = 0;
        while offset < wal_bytes.len() {
            match crate::db::wal::format::decode_at(wal_bytes, offset) {
                Ok(Some((record, next))) => {
                    match record.kind {
                        RecordKind::Put => {
                            let user_key =
                                encode_user_key_smallvec(&record.namespace, &record.key)?;
                            let (stored, externalized) =
                                externalize_value_for_memtable(&mut self.blob_store, record.value);
                            if externalized {
                                self.jupiter_telemetry.blob_values_externalized = self
                                    .jupiter_telemetry
                                    .blob_values_externalized
                                    .saturating_add(1);
                            }
                            self.memtable.apply(&user_key, record.version, Some(stored));
                            self.clock.observe_packed(record.version);
                            self.uncertainty.observe_remote_packed(record.version);
                            self.watermarks.observe(LOCAL_NODE_ID, record.version);
                            let ns = String::from_utf8_lossy(&record.namespace).into_owned();
                            self.safe_time.observe_shard_safe_time_no_recompute(
                                ns,
                                LOCAL_REGION_ID,
                                record.version,
                            );
                            max_applied_version =
                                Some(max_applied_version.unwrap_or(0).max(record.version));
                        }
                        RecordKind::Delete => {
                            let user_key =
                                encode_user_key_smallvec(&record.namespace, &record.key)?;
                            self.memtable.apply(&user_key, record.version, None);
                            self.clock.observe_packed(record.version);
                            self.uncertainty.observe_remote_packed(record.version);
                            self.watermarks.observe(LOCAL_NODE_ID, record.version);
                            let ns = String::from_utf8_lossy(&record.namespace).into_owned();
                            self.safe_time.observe_shard_safe_time_no_recompute(
                                ns,
                                LOCAL_REGION_ID,
                                record.version,
                            );
                            max_applied_version =
                                Some(max_applied_version.unwrap_or(0).max(record.version));
                        }
                        RecordKind::RaftMeta | RecordKind::Unknown(_) => {
                            // Skip non-data records for memtable apply.
                        }
                    }
                    offset = next;
                }
                Ok(None) => {
                    return Err(DbError::io(
                        "follower WAL direct decode failed: truncated WAL record".to_string(),
                    ));
                }
                Err(e) => {
                    return Err(DbError::io(format!(
                        "follower WAL direct decode failed: {e}"
                    )));
                }
            }
        }
        if let Some(version) = max_applied_version {
            self.safe_time
                .observe_shard_safe_time_no_recompute("clock", LOCAL_REGION_ID, version);
            self.safe_time.recompute_region_safe_times();
        }
        self.mark_lsm_stats_dirty();
        Ok(max_applied_version.unwrap_or(0))
    }

    fn install_sorted_run_chunk(
        &mut self,
        term: u64,
        chunk_stream_id: u64,
        chunk_index: u64,
        total_chunks: u64,
        payload: Vec<u8>,
    ) -> SortedRunCatchUpChunkInstallStatus {
        self.jupiter_telemetry.sorted_run_catchup_requests = self
            .jupiter_telemetry
            .sorted_run_catchup_requests
            .saturating_add(1);
        let reject = |next_chunk_index: u64, reason: String| SortedRunCatchUpChunkInstallStatus {
            accepted: false,
            next_chunk_index,
            rejection_reason: Some(reason),
        };
        if total_chunks == 0 {
            return reject(0, "SORTED_RUN_INVALID_TOTAL_CHUNKS".to_string());
        }
        if chunk_index >= total_chunks {
            return reject(0, "SORTED_RUN_INVALID_CHUNK_INDEX".to_string());
        }
        if term < self.raft_current_term {
            return reject(0, "SORTED_RUN_STALE_TERM_REJECTED".to_string());
        }

        let (next_chunk_index, completed) = {
            let state = self
                .sorted_run_installs
                .entry(chunk_stream_id)
                .or_insert_with(|| SortedRunInstallState::new(term, total_chunks));
            if term > state.term {
                *state = SortedRunInstallState::new(term, total_chunks);
            }
            if term < state.term {
                return reject(
                    state.next_chunk_index,
                    "SORTED_RUN_STALE_STREAM_TERM_REJECTED".to_string(),
                );
            }
            if total_chunks != state.total_chunks {
                return reject(
                    state.next_chunk_index,
                    "SORTED_RUN_STREAM_TOTAL_CHUNKS_MISMATCH".to_string(),
                );
            }
            if chunk_index < state.next_chunk_index {
                let payload_hash = payload_hash64(&payload);
                if let Some(stored_hash) = state.chunk_hashes.get(&chunk_index) {
                    if *stored_hash == payload_hash {
                        return SortedRunCatchUpChunkInstallStatus {
                            accepted: true,
                            next_chunk_index: state.next_chunk_index,
                            rejection_reason: None,
                        };
                    }
                    return reject(
                        state.next_chunk_index,
                        "SORTED_RUN_DUPLICATE_CHUNK_PAYLOAD_MISMATCH".to_string(),
                    );
                }
                return reject(
                    state.next_chunk_index,
                    "SORTED_RUN_DUPLICATE_CHUNK_HISTORY_MISSING".to_string(),
                );
            }
            if chunk_index > state.next_chunk_index {
                return reject(
                    state.next_chunk_index,
                    "SORTED_RUN_OUT_OF_ORDER_CHUNK".to_string(),
                );
            }
            if crate::db::lsm::sstable::decode_block(&payload).is_err() {
                return reject(
                    state.next_chunk_index,
                    "SORTED_RUN_CHUNK_INVALID_ENCODING".to_string(),
                );
            }
            state.chunk_payloads.insert(chunk_index, payload.clone());
            state
                .chunk_hashes
                .insert(chunk_index, payload_hash64(payload.as_slice()));
            state.next_chunk_index = state.next_chunk_index.saturating_add(1);
            (
                state.next_chunk_index,
                state.next_chunk_index >= state.total_chunks,
            )
        };

        if completed {
            let Some(state) = self.sorted_run_installs.remove(&chunk_stream_id) else {
                return reject(
                    next_chunk_index,
                    "SORTED_RUN_STREAM_STATE_MISSING".to_string(),
                );
            };
            for idx in 0..state.total_chunks {
                let Some(chunk_payload) = state.chunk_payloads.get(&idx) else {
                    return reject(next_chunk_index, "SORTED_RUN_STREAM_INCOMPLETE".to_string());
                };
                if let Err(reason) = self.apply_sorted_run_chunk_payload(chunk_payload) {
                    return reject(next_chunk_index, reason);
                }
            }
            self.jupiter_telemetry.sorted_run_catchup_chunks_applied = self
                .jupiter_telemetry
                .sorted_run_catchup_chunks_applied
                .saturating_add(state.total_chunks);
        }
        SortedRunCatchUpChunkInstallStatus {
            accepted: true,
            next_chunk_index,
            rejection_reason: None,
        }
    }

    fn wal_flush_stats(&self) -> DbWalFlushStats {
        self.wal.flush_stats().into()
    }

    fn wal_for_lane(&self, lane_id: usize) -> Arc<WalSegment> {
        if self.lane_wals.len() > 1 {
            self.lane_wals
                .get(lane_id)
                .cloned()
                .unwrap_or_else(|| self.wal.clone())
        } else {
            self.wal.clone()
        }
    }

    fn commit_visibility_status(&self) -> DbCommitVisibilityStatus {
        let primary = self.primary_replication();
        let apply_backlog_depth = primary
            .durability_commit_index
            .saturating_sub(primary.apply_visible_index);
        DbCommitVisibilityStatus {
            mode: self.commit_visibility_mode,
            durability_commit_index: primary.durability_commit_index,
            apply_visible_index: primary.apply_visible_index,
            apply_backlog_depth,
        }
    }

    fn mark_group_durable(&mut self, active_group_id: u32, required_index: u64) {
        if let Ok(replication) = self.replication_for_group_mut(active_group_id) {
            replication.durability_commit_index =
                replication.durability_commit_index.max(required_index);
            let backlog = replication
                .durability_commit_index
                .saturating_sub(replication.apply_visible_index);
            self.apply_backlog_peak = self.apply_backlog_peak.max(backlog);
        }
    }

    fn mark_group_apply_visible(&mut self, active_group_id: u32, required_index: u64) {
        if let Ok(replication) = self.replication_for_group_mut(active_group_id) {
            replication.apply_visible_index = replication.apply_visible_index.max(required_index);
        }
    }

    fn apply_committed_task(&mut self, task: ApplyTask) {
        self.apply_staged_ops(&task.staged_ops);
        self.mark_group_apply_visible(task.active_group_id, task.required_index);
    }

    fn checkpoint_tick_background(&mut self) {
        let now = now_epoch_s();
        if now.saturating_sub(self.checkpoint_last_epoch_s) >= self.checkpoint_interval_secs {
            let _ = self.checkpoint_create();
            self.checkpoint_last_epoch_s = now;
        }
    }

    fn persist_raft_state_now(&self) -> Result<(), DbError> {
        let primary = self.primary_replication();
        let persisted = PersistedRaftState::capture(&primary.leader, &primary.membership);
        persist_raft_state(&self.wal_path, &persisted).map_err(|err| DbError::io(err.to_string()))
    }

    fn capture_persisted_topology(&self) -> PersistedTopologyState {
        let mut groups = Vec::with_capacity(self.replication_groups.len());
        let mut ordered = self.replication_groups.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|(group_id, _)| **group_id);
        for (group_id, replication) in ordered {
            groups.push(PersistedGroupState {
                group_id: *group_id,
                raft: PersistedRaftState::capture(&replication.leader, &replication.membership),
            });
        }
        let autoscale_status = Some(PersistedAutoscaleStatus {
            last_action: self.autoscale_status.last_action.clone(),
            reasons: self.autoscale_status.reasons.clone(),
            last_action_at_epoch_ms: self.autoscale_status.last_action_at_epoch_ms,
        });
        PersistedTopologyState::new(
            self.shard_directory.snapshot(),
            groups,
            self.replication_factor,
            self.write_quorum,
            autoscale_status,
        )
    }

    fn persist_topology_state_now(&self) -> Result<(), DbError> {
        let persisted = self.capture_persisted_topology();
        persist_topology_state(&self.wal_path, &persisted)
            .map_err(|err| DbError::io(err.to_string()))
    }

    fn persist_topology_state_best_effort(&mut self) {
        match self.persist_topology_state_now() {
            Ok(()) => {
                self.topology_state_dirty = false;
            }
            Err(_) => {
                self.topology_state_dirty = true;
            }
        }
    }

    fn persist_topology_state_required(&mut self) -> Result<(), DbError> {
        match self.persist_topology_state_now() {
            Ok(()) => {
                self.topology_state_dirty = false;
                Ok(())
            }
            Err(err) => {
                self.topology_state_dirty = true;
                Err(err)
            }
        }
    }

    fn persist_raft_state_required(&mut self) -> Result<(), DbError> {
        match self.persist_raft_state_now() {
            Ok(()) => {
                self.raft_persist_error = None;
                self.raft_persist_error_at = None;
                self.raft_persist_ops_since_flush = 0;
                let requires_topology_flush = self.topology_state_dirty
                    || self.shard_directory.active_group_count() > PRIMARY_ACTIVE_GROUP_ID + 1;
                if requires_topology_flush {
                    self.persist_topology_state_required()?;
                }
                Ok(())
            }
            Err(err) => {
                self.raft_persist_error = Some(err.message.clone());
                self.raft_persist_error_at = Some(now_epoch_s());
                Err(err)
            }
        }
    }

    #[allow(dead_code)]
    fn maybe_persist_raft_state_required(&mut self) -> Result<(), DbError> {
        if self.raft_persist_interval_ops <= 1 {
            return self.persist_raft_state_required();
        }
        self.raft_persist_ops_since_flush = self.raft_persist_ops_since_flush.saturating_add(1);
        if self.raft_persist_ops_since_flush >= self.raft_persist_interval_ops {
            self.persist_raft_state_required()
        } else {
            Ok(())
        }
    }

    /// If the Raft persist interval has been reached, prepare metadata for
    /// appending as a RaftMeta WAL record. Does bookkeeping. When topology
    /// persist is required, captures state for deferred persist instead of
    /// doing it under the lock. Caller must append the record to staged_records
    /// before WAL append so it is covered by the same fsync.
    fn maybe_raft_meta_for_wal(
        &mut self,
        active_group_id: u32,
        required_index: u64,
    ) -> (
        Option<RaftPersistMetadata>,
        Option<(PathBuf, PersistedTopologyState)>,
    ) {
        self.raft_persist_ops_since_flush = self.raft_persist_ops_since_flush.saturating_add(1);
        if self.raft_persist_interval_ops <= 1
            || self.raft_persist_ops_since_flush >= self.raft_persist_interval_ops
        {
            let primary = self.primary_replication();
            let new_commit_index = if active_group_id == PRIMARY_ACTIVE_GROUP_ID {
                required_index
            } else {
                primary.leader.commit_index
            };
            let metadata = RaftPersistMetadata {
                current_term: primary.leader.current_term,
                voted_for: primary.leader.voted_for,
                commit_index: new_commit_index,
                needs_membership_flush: self.topology_state_dirty,
            };

            self.raft_persist_ops_since_flush = 0;
            self.raft_persist_error = None;
            self.raft_persist_error_at = None;
            let requires_topology = self.topology_state_dirty
                || self.shard_directory.active_group_count() > PRIMARY_ACTIVE_GROUP_ID + 1;
            let deferred_topology = if requires_topology {
                self.topology_state_dirty = false;
                Some((self.wal_path.clone(), self.capture_persisted_topology()))
            } else {
                None
            };
            (Some(metadata), deferred_topology)
        } else {
            (None, None)
        }
    }

    /// Capture a clock persist snapshot if the persist interval has
    /// been reached. Bookkeeping is updated under the lock.
    fn maybe_capture_deferred_clock_persist(&mut self, packed: u64) -> Option<(PathBuf, u64)> {
        self.clock_persist_ops_since_flush = self.clock_persist_ops_since_flush.saturating_add(1);
        if self.clock_persist_interval_ops <= 1
            || self.clock_persist_ops_since_flush >= self.clock_persist_interval_ops
        {
            self.clock_persist_ops_since_flush = 0;
            self.clock_persist_error = None;
            self.clock_persist_error_at = None;
            Some((self.wal_path.clone(), packed))
        } else {
            None
        }
    }

    pub fn health_status(&mut self) -> DbHealthStatus {
        let memtable_stats = self.lsm_stats_for_health_snapshot();
        let primary = self.primary_replication();
        let apply_backlog_depth = primary
            .durability_commit_index
            .saturating_sub(primary.apply_visible_index);
        let rpc_snapshot = crate::db::rpc::private_network::replication_rpc_in_flight_snapshot();
        let to_bps = |num: u64, den: u64| -> u64 {
            if den == 0 {
                0
            } else {
                num.saturating_mul(10_000) / den
            }
        };
        let replication_contact_efficiency_bps = to_bps(
            self.replication_telemetry.last_successful_count,
            self.replication_telemetry.last_contacted_count,
        );
        let replication_target_efficiency_bps = to_bps(
            self.replication_telemetry.last_successful_count,
            self.replication_telemetry.last_target_count,
        );
        let replication_failure_counters = self
            .replication_telemetry
            .failure_counters
            .iter()
            .map(|(token, count)| DbFailureCounter {
                token: token.clone(),
                count: *count,
            })
            .collect();
        DbHealthStatus {
            clock_persist_error: self.clock_persist_error.clone(),
            clock_persist_error_at: self.clock_persist_error_at,
            raft_persist_error: self.raft_persist_error.clone(),
            raft_persist_error_at: self.raft_persist_error_at,
            cdc_checkpoint_persist_error: self.cdc_checkpoint_persist_error.clone(),
            cdc_checkpoint_persist_error_at: self.cdc_checkpoint_persist_error_at,
            checkpoint_persist_error: self.checkpoint_persist_error.clone(),
            checkpoint_persist_error_at: self.checkpoint_persist_error_at,
            checkpoint_restore_error: self.checkpoint_restore_error.clone(),
            checkpoint_restore_error_at: self.checkpoint_restore_error_at,
            schema_gate_error: self.schema_gate_error.clone(),
            schema_gate_error_at: self.schema_gate_error_at,
            replication_queue_depth: self.replication_telemetry.queue_depth,
            replication_queue_depth_peak: self.replication_telemetry.queue_depth_peak,
            replication_batch_samples: self.replication_telemetry.batch_samples,
            replication_batch_ops_le_1: self.replication_telemetry.batch_ops_le_1,
            replication_batch_ops_le_4: self.replication_telemetry.batch_ops_le_4,
            replication_batch_ops_le_16: self.replication_telemetry.batch_ops_le_16,
            replication_batch_ops_le_64: self.replication_telemetry.batch_ops_le_64,
            replication_batch_ops_gt_64: self.replication_telemetry.batch_ops_gt_64,
            replication_batch_bytes_le_1k: self.replication_telemetry.batch_bytes_le_1k,
            replication_batch_bytes_le_4k: self.replication_telemetry.batch_bytes_le_4k,
            replication_batch_bytes_le_16k: self.replication_telemetry.batch_bytes_le_16k,
            replication_batch_bytes_le_64k: self.replication_telemetry.batch_bytes_le_64k,
            replication_batch_bytes_gt_64k: self.replication_telemetry.batch_bytes_gt_64k,
            quorum_ack_count: self.replication_telemetry.last_quorum_acks,
            quorum_size: self.replication_telemetry.last_quorum_size,
            quorum_replication_latency_ns: self
                .replication_telemetry
                .last_quorum_replication_latency_ns,
            quorum_fsync_latency_ns: self.replication_telemetry.last_quorum_fsync_latency_ns,
            quorum_failure_token: self.replication_telemetry.last_failure_token.clone(),
            quorum_failure_reason: self.replication_telemetry.last_failure_reason.clone(),
            replica_acks: self.replication_telemetry.last_replica_acks.clone(),
            replication_target_count: self.replication_telemetry.last_target_count,
            replication_contacted_count: self.replication_telemetry.last_contacted_count,
            replication_wave_count: self.replication_telemetry.last_wave_count,
            replication_wave_avg_targets: self.replication_telemetry.last_wave_avg_targets,
            replication_wave_max_targets: self.replication_telemetry.last_wave_max_targets,
            replication_successful_count: self.replication_telemetry.last_successful_count,
            replication_failed_count: self.replication_telemetry.last_failed_count,
            replication_cancelled_count: self.replication_telemetry.last_cancelled_count,
            replication_contact_efficiency_bps,
            replication_target_efficiency_bps,
            replication_skipped_count: self.replication_telemetry.last_skipped_count,
            replication_aborted_in_flight_count: self
                .replication_telemetry
                .last_aborted_in_flight_count,
            replication_failure_counters,
            replication_simulation_commits: self.replication_telemetry.simulation_commits,
            replication_rpc_max_in_flight: rpc_snapshot.max_in_flight,
            replication_rpc_in_flight: rpc_snapshot.in_flight,
            replication_rpc_available_permits: rpc_snapshot.available_permits,
            replication_rpc_backpressure_timeouts: rpc_snapshot.backpressure_timeouts,
            replication_rpc_backpressure_closed: rpc_snapshot.backpressure_closed,
            quorum_transport_mode: self.quorum_transport_mode,
            writer_lanes: Vec::new(),
            writer_lane_max_enqueue_share_bps: 0,
            writer_lane_max_retry_after_bps: 0,
            writer_lane_max_saturation_bps: 0,
            writer_lane_assignment_lookups: 0,
            writer_lane_assignment_hits: 0,
            writer_lane_assignment_misses: 0,
            writer_lane_assignment_hit_rate_bps: 0,
            apply_lanes: Vec::new(),
            apply_lane_max_queue_depth: 0,
            replicated_log_backend: self.replicated_log_backend,
            replicated_log_shadow_payload_bytes: self.replicated_log_telemetry.payload_bytes,
            replicated_log_shadow_wal_bytes: self.replicated_log_telemetry.wal_bytes,
            replicated_log_shadow_overhead_bytes: self.replicated_log_telemetry.overhead_bytes(),
            apply_backlog_depth,
            apply_backlog_peak: self.apply_backlog_peak.max(apply_backlog_depth),
            lsm_compaction_debt_bytes_estimate: memtable_stats.compaction_debt_bytes_estimate,
            lsm_shadow_bytes_estimate: memtable_stats.shadow_bytes_estimate,
            lsm_live_bytes_estimate: memtable_stats.live_bytes_estimate,
            lsm_total_bytes_estimate: memtable_stats.total_bytes_estimate,
            lsm_version_count: memtable_stats.version_count,
            lsm_tombstone_count: memtable_stats.tombstone_count,
            replication_outside_lock_active: replication_outside_lock_active(),
            wal_encode_outside_lock_active: wal_encode_outside_lock_active(),
            sorted_run_catchup_active: sorted_run_catchup_active(),
            sorted_run_catchup_lag_threshold_ops: sorted_run_catchup_lag_threshold_ops_default(),
            sorted_run_catchup_requests: self.jupiter_telemetry.sorted_run_catchup_requests,
            sorted_run_catchup_chunks_sent: self.jupiter_telemetry.sorted_run_catchup_chunks_sent,
            sorted_run_catchup_chunks_applied: self
                .jupiter_telemetry
                .sorted_run_catchup_chunks_applied,
            compaction_scheduler_active: compaction_scheduler_active(),
            compaction_scheduler_max_debt_bytes: compaction_scheduler_max_debt_bytes_default(),
            compaction_scheduler_admitted: self.jupiter_telemetry.compaction_scheduler_admitted,
            compaction_scheduler_deferred: self.jupiter_telemetry.compaction_scheduler_deferred,
            compaction_scheduler_rejected: self.jupiter_telemetry.compaction_scheduler_rejected,
            blob_value_threshold_bytes: blob_value_threshold_bytes_default() as u64,
            blob_gc_active: blob_gc_active(),
            blob_values_externalized: self.jupiter_telemetry.blob_values_externalized,
            blob_gc_runs: self.jupiter_telemetry.blob_gc_runs,
            blob_gc_reclaimed_bytes: self.jupiter_telemetry.blob_gc_reclaimed_bytes,
            insert_fast_lane_active: insert_fast_lane_active(),
            insert_fast_lane_accepted: self.jupiter_telemetry.insert_fast_lane_accepted,
            insert_fast_lane_rejected: self.jupiter_telemetry.insert_fast_lane_rejected,
            latency_frontier_mode_active: latency_frontier_mode_active(),
            frontier_speculative_plans: self.jupiter_telemetry.frontier_speculative_plans,
            frontier_wave_plans: self.jupiter_telemetry.frontier_wave_plans,
            memtable_gc_enabled: MEMTABLE_GC_ENABLED,
            memtable_gc_runs: self.jupiter_telemetry.memtable_gc_runs,
            memtable_gc_versions_dropped: self.jupiter_telemetry.memtable_gc_versions_dropped,
            memtable_gc_tombstone_keys_removed: self
                .jupiter_telemetry
                .memtable_gc_tombstone_keys_removed,
        }
    }

    fn evaluate_schema_gate_for_epoch(&mut self, target_epoch: u64) -> Result<(), DbError> {
        let mut voter_ranges = Vec::new();
        let min_supported = self.schema_committed_epoch.saturating_sub(1);
        let max_supported = self.schema_committed_epoch.saturating_add(1);
        for _ in self.primary_replication().membership.voters() {
            voter_ranges.push(crate::db::schema_gate::SchemaEpochRange {
                min_supported: crate::db::schema_gate::SchemaEpoch(min_supported),
                max_supported: crate::db::schema_gate::SchemaEpoch(max_supported),
            });
        }

        let decision = crate::db::schema_gate::evaluate_schema_gate(
            &crate::db::schema_gate::SchemaGateInput {
                mode: crate::db::schema_gate::SchemaCompatibilityMode::ExpandContract,
                committed_epoch: crate::db::schema_gate::SchemaEpoch(self.schema_committed_epoch),
                target_write_epoch: crate::db::schema_gate::SchemaEpoch(target_epoch),
                voter_ranges,
                all_voters_on_target_binary: self.schema_all_voters_on_target_binary,
            },
        );

        match decision {
            crate::db::schema_gate::SchemaGateDecision::Allow => {
                self.schema_gate_error = None;
                self.schema_gate_error_at = None;
                Ok(())
            }
            crate::db::schema_gate::SchemaGateDecision::Deny { reason } => {
                self.schema_gate_error = Some(reason.clone());
                self.schema_gate_error_at = Some(now_epoch_s());
                Err(DbError::limit(format!("SCHEMA_WRITE_GATE: {reason}")))
            }
        }
    }

    fn checkpoint_create(&mut self) -> Result<crate::db::checkpoint::CheckpointInfo, DbError> {
        self.enforce_checkpoint_region_policy()?;
        let Some(data_dir) = self.wal_path.parent() else {
            return Err(DbError::io("missing data dir for checkpoint"));
        };
        let manager = self.checkpoint_manager()?;
        match manager.create_checkpoint(data_dir) {
            Ok(info) => {
                self.checkpoint_persist_error = None;
                self.checkpoint_persist_error_at = None;
                Ok(info)
            }
            Err(err) => {
                self.checkpoint_persist_error = Some(err.to_string());
                self.checkpoint_persist_error_at = Some(now_epoch_s());
                Err(DbError::io(err.to_string()))
            }
        }
    }

    fn checkpoint_restore_latest(
        &mut self,
    ) -> Result<crate::db::checkpoint::CheckpointInfo, DbError> {
        self.enforce_checkpoint_region_policy()?;
        let Some(data_dir) = self.wal_path.parent() else {
            return Err(DbError::io("missing data dir for restore"));
        };
        let manager = self.checkpoint_manager()?;
        match manager.restore_latest(data_dir) {
            Ok(info) => {
                self.checkpoint_restore_error = None;
                self.checkpoint_restore_error_at = None;
                Ok(info)
            }
            Err(err) => {
                self.checkpoint_restore_error = Some(err.to_string());
                self.checkpoint_restore_error_at = Some(now_epoch_s());
                Err(DbError::io(err.to_string()))
            }
        }
    }

    fn checkpoint_restore_by_id(
        &mut self,
        checkpoint_id: &str,
    ) -> Result<crate::db::checkpoint::CheckpointInfo, DbError> {
        self.enforce_checkpoint_region_policy()?;
        let Some(data_dir) = self.wal_path.parent() else {
            return Err(DbError::io("missing data dir for restore"));
        };
        let manager = self.checkpoint_manager()?;
        match manager.restore_checkpoint(data_dir, checkpoint_id) {
            Ok(info) => {
                self.checkpoint_restore_error = None;
                self.checkpoint_restore_error_at = None;
                Ok(info)
            }
            Err(err) => {
                self.checkpoint_restore_error = Some(err.to_string());
                self.checkpoint_restore_error_at = Some(now_epoch_s());
                Err(DbError::io(err.to_string()))
            }
        }
    }

    fn checkpoint_list(&self) -> Result<Vec<crate::db::checkpoint::CheckpointInfo>, DbError> {
        let manager = self.checkpoint_manager()?;
        manager
            .list_checkpoints()
            .map_err(|err| DbError::io(err.to_string()))
    }

    fn checkpoint_prune(&self, retain: usize) -> Result<(), DbError> {
        let manager = self.checkpoint_manager()?;
        manager
            .prune_remote(retain)
            .map_err(|err| DbError::io(err.to_string()))?;
        manager
            .prune_local()
            .map_err(|err| DbError::io(err.to_string()))
    }

    fn checkpoint_manager(&self) -> Result<crate::db::checkpoint::CheckpointManager, DbError> {
        self.checkpoint_manager.clone().map(Ok).unwrap_or_else(|| {
            self.checkpoint_config
                .build_manager()
                .map_err(|err| DbError::io(err.to_string()))
        })
    }

    fn validate_batch(batch: &[BatchOp]) -> Result<(), DbError> {
        if batch.is_empty() {
            return Err(DbError::invalid_argument("empty batch"));
        }
        if batch.len() > MAX_BATCH_OPS {
            return Err(DbError::limit(format!(
                "batch op count {} exceeds {}",
                batch.len(),
                MAX_BATCH_OPS
            )));
        }

        let mut total_bytes = 0usize;
        for op in batch {
            match op {
                BatchOp::Put {
                    namespace,
                    key,
                    value,
                    ..
                } => {
                    if key.len() > MAX_KEY_BYTES {
                        return Err(DbError::limit("key exceeds MAX_KEY_BYTES"));
                    }
                    if value.len() > MAX_VALUE_BYTES {
                        return Err(DbError::limit("value exceeds MAX_VALUE_BYTES"));
                    }
                    total_bytes += namespace.len() + key.len() + value.len();
                }
                BatchOp::Delete { namespace, key, .. } => {
                    if key.len() > MAX_KEY_BYTES {
                        return Err(DbError::limit("key exceeds MAX_KEY_BYTES"));
                    }
                    total_bytes += namespace.len() + key.len();
                }
            }
        }
        if total_bytes > MAX_BATCH_BYTES {
            return Err(DbError::limit(format!(
                "batch bytes {} exceeds {}",
                total_bytes, MAX_BATCH_BYTES
            )));
        }
        Ok(())
    }

    fn route_key_to_shard(&self, namespace: &[u8], key: &[u8]) -> Result<ShardRoute, DbError> {
        self.shard_directory
            .route_key(namespace, key)
            .map_err(|err| DbError::invalid_argument(format!("shard route failed: {err:?}")))
    }

    fn route_batch_to_shard(&self, batch: &[BatchOp]) -> Result<ShardRoute, DbError> {
        match self.shard_directory.route_batch(batch) {
            Ok(route) => Ok(route),
            Err(ShardDirectoryError::RouteMiss) => Err(DbError::mixed_shard_batch(
                "MIXED_SHARD_BATCH_UNSUPPORTED: batch must target exactly one logical shard",
            )),
            Err(err) => Err(DbError::invalid_argument(format!(
                "shard route failed: {err:?}"
            ))),
        }
    }

    fn keyrange_id_for_shard(logical_shard_id: u32) -> String {
        format!("kr:shard:{logical_shard_id}")
    }

    fn ownership_token_for(
        keyrange_id: &str,
        sovereignty_id: &str,
        home_region: &str,
        home_epoch: u64,
        shard_map_epoch: u64,
        leader_node_id: &str,
    ) -> String {
        let mut hasher = DefaultHasher::new();
        keyrange_id.hash(&mut hasher);
        sovereignty_id.hash(&mut hasher);
        home_region.hash(&mut hasher);
        home_epoch.hash(&mut hasher);
        shard_map_epoch.hash(&mut hasher);
        leader_node_id.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    fn ownership_locality_policy(&self) -> crate::db::placement::ResidencyPolicy {
        let mut canonical_regions = self.topology_canonical_regions.clone();
        if canonical_regions.is_empty() {
            canonical_regions.insert(self.local_region.clone());
        }
        let sovereignty_regions = self
            .sovereignty_allowed_regions
            .iter()
            .filter_map(|region| normalize_region_id(region))
            .collect::<BTreeSet<_>>();
        let mut allow_localities = if sovereignty_regions.is_empty() {
            canonical_regions.clone()
        } else {
            let filtered = canonical_regions
                .intersection(&sovereignty_regions)
                .cloned()
                .collect::<BTreeSet<_>>();
            if filtered.is_empty() {
                canonical_regions.clone()
            } else {
                filtered
            }
        };
        if allow_localities.is_empty() {
            allow_localities.insert(self.local_region.clone());
        }
        if !self.sovereignty_enforce_all_copies && canonical_regions.contains(&self.local_region) {
            allow_localities.insert(self.local_region.clone());
        }
        crate::db::placement::ResidencyPolicy {
            scope: self.sovereignty_id.clone(),
            allow_localities,
            deny_localities: BTreeSet::new(),
        }
    }

    fn sync_keyrange_ownership_state(&mut self) -> Result<(), DbError> {
        let shard_map_epoch = self.shard_directory.epoch();
        let mut live = BTreeSet::new();
        for shard in self.shard_directory.shards() {
            let keyrange_id = Self::keyrange_id_for_shard(shard.shard_id);
            live.insert(keyrange_id.clone());
            let policy = self.ownership_locality_policy();
            if self.home_store.get_home(&keyrange_id).is_none() {
                let initial_home = if policy.allow_localities.contains(&self.local_region) {
                    self.local_region.clone()
                } else {
                    policy
                        .allow_localities
                        .iter()
                        .next()
                        .cloned()
                        .ok_or_else(|| {
                            DbError::invalid_argument(format!(
                                "OWNERSHIP_POLICY_EMPTY: keyrange={keyrange_id}"
                            ))
                        })?
                };
                self.home_store
                    .set_home(&keyrange_id, &initial_home, &policy)
                    .map_err(|err| {
                        DbError::invalid_argument(format!(
                            "HOME_SET_FAILED: keyrange={keyrange_id} err={err:?}"
                        ))
                    })?;
            }
            let home_region = self
                .home_store
                .get_home(&keyrange_id)
                .unwrap_or_else(|| {
                    if policy.allow_localities.contains(&self.local_region) {
                        self.local_region.as_str()
                    } else {
                        policy
                            .allow_localities
                            .iter()
                            .next()
                            .map(String::as_str)
                            .unwrap_or(self.local_region.as_str())
                    }
                })
                .to_string();
            if self.sovereignty_enforce_all_copies
                && !policy.allow_localities.contains(&home_region)
            {
                return Err(DbError::invalid_argument(format!(
                    "OWNERSHIP_HOME_OUTSIDE_SOVEREIGNTY: keyrange={keyrange_id} home={home_region} sovereignty={}",
                    self.sovereignty_id
                )));
            }
            let mut async_regions = BTreeSet::new();
            if self.replication_async_failover {
                async_regions.extend(
                    policy
                        .allow_localities
                        .iter()
                        .filter(|region| region.as_str() != home_region.as_str())
                        .cloned(),
                );
            }
            let entry = self
                .keyrange_ownership
                .entry(keyrange_id.clone())
                .or_insert_with(|| KeyrangeOwnershipState {
                    keyrange_id: keyrange_id.clone(),
                    sovereignty_id: self.sovereignty_id.clone(),
                    home_region: home_region.clone(),
                    home_epoch: 1,
                    leader_node_id: "local".to_string(),
                    ownership_token: String::new(),
                    async_failover_regions: async_regions.clone(),
                });
            entry.sovereignty_id = self.sovereignty_id.clone();
            entry.home_region = home_region;
            entry.async_failover_regions = async_regions;
            entry.ownership_token = Self::ownership_token_for(
                &entry.keyrange_id,
                &entry.sovereignty_id,
                &entry.home_region,
                entry.home_epoch,
                shard_map_epoch,
                &entry.leader_node_id,
            );
        }
        self.keyrange_ownership
            .retain(|keyrange_id, _| live.contains(keyrange_id));
        Ok(())
    }

    fn owner_record_for_route(&self, route: &ShardRoute) -> Result<OwnerRecord, DbError> {
        let keyrange_id = Self::keyrange_id_for_shard(route.logical_shard_id);
        let state = self.keyrange_ownership.get(&keyrange_id).ok_or_else(|| {
            DbError::invalid_argument(format!("OWNER_RECORD_MISSING: keyrange={keyrange_id}"))
        })?;
        Ok(OwnerRecord {
            keyrange_id: state.keyrange_id.clone(),
            sovereignty_id: state.sovereignty_id.clone(),
            home_region: state.home_region.clone(),
            home_epoch: state.home_epoch,
            leader_node_id: state.leader_node_id.clone(),
            ownership_token: state.ownership_token.clone(),
            shard_map_epoch: self.shard_directory.epoch(),
            async_failover_regions: state.async_failover_regions.iter().cloned().collect(),
        })
    }

    fn current_ownership_fence_for_route(
        &self,
        route: &ShardRoute,
    ) -> Result<OwnershipFence, DbError> {
        let owner = self.owner_record_for_route(route)?;
        Ok(OwnershipFence {
            expected_home_epoch: owner.home_epoch,
            expected_shard_map_epoch: owner.shard_map_epoch,
            ownership_token: owner.ownership_token,
        })
    }

    fn enforce_ownership_fence_for_route(
        &self,
        route: &ShardRoute,
        fence: &OwnershipFence,
    ) -> Result<(), DbError> {
        let owner = self.owner_record_for_route(route)?;
        if fence.expected_shard_map_epoch != owner.shard_map_epoch {
            return Err(DbError::invalid_argument(format!(
                "DIRECTORY_EPOCH_STALE: expected_shard_map_epoch={} actual={}",
                fence.expected_shard_map_epoch, owner.shard_map_epoch
            )));
        }
        if fence.expected_home_epoch != owner.home_epoch {
            return Err(DbError::invalid_argument(format!(
                "HOME_EPOCH_FENCE_VIOLATION: keyrange={} expected={} actual={}",
                owner.keyrange_id, fence.expected_home_epoch, owner.home_epoch
            )));
        }
        if fence.ownership_token != owner.ownership_token {
            return Err(DbError::invalid_argument(format!(
                "OWNERSHIP_TOKEN_FENCE_VIOLATION: keyrange={}",
                owner.keyrange_id
            )));
        }
        Ok(())
    }

    fn authorize_write_namespace(&self, namespace: &[u8]) -> Result<(), DbError> {
        if namespace == IDEMPOTENCY_NAMESPACE {
            return Ok(());
        }
        if let Some(policy) = &self.residency_policy {
            policy
                .authorize_write(namespace, &self.local_region)
                .map_err(|err| DbError::sovereignty_write_denied(err.fail_closed_message()))?;
        }
        Ok(())
    }

    fn authorize_read_namespace(
        &self,
        namespace: &[u8],
        mode: ReadSovereigntyMode,
        consistency: ReadConsistency,
    ) -> Result<(), DbError> {
        if namespace == IDEMPOTENCY_NAMESPACE {
            return Ok(());
        }
        let Some(policy) = &self.residency_policy else {
            return Ok(());
        };
        let effective_mode = policy
            .authorize_read(namespace, &self.local_region, mode)
            .map_err(|err| DbError::sovereignty_read_denied(err.fail_closed_message()))?;
        if effective_mode == ReadSovereigntyMode::StaleOk && consistency == ReadConsistency::Strong
        {
            return Err(DbError::sovereignty_read_denied(
                "SOVEREIGNTY_READ_DENIED: stale_ok mode requires eventual consistency",
            ));
        }
        Ok(())
    }

    fn enforce_checkpoint_region_policy(&self) -> Result<(), DbError> {
        if !self.checkpoint_allowed_regions.is_empty()
            && !self
                .checkpoint_allowed_regions
                .iter()
                .any(|region| region == &self.local_region)
        {
            return Err(DbError::sovereignty_checkpoint_denied(format!(
                "SOVEREIGNTY_CHECKPOINT_REGION_DENIED: region={} allowed={:?}",
                self.local_region, self.checkpoint_allowed_regions
            )));
        }
        if let Some(policy) = &self.residency_policy {
            policy
                .authorize_checkpoint_region(&self.local_region)
                .map_err(|err| DbError::sovereignty_checkpoint_denied(err.fail_closed_message()))?;
        }
        Ok(())
    }

    fn bind_txn_to_shard(&mut self, txn_id: u64, shard_id: u32) -> Result<(), DbError> {
        let record = self
            .txns
            .get_mut(&txn_id)
            .ok_or_else(|| DbError::invalid_argument("unknown txn id"))?;
        match record.bound_shard {
            Some(existing) if existing != shard_id => Err(DbError::cross_shard_txn(format!(
                "CROSS_SHARD_TXN_UNSUPPORTED: txn_id={txn_id} bound_shard={existing} requested_shard={shard_id}"
            ))),
            Some(_) => Ok(()),
            None => {
                record.bound_shard = Some(shard_id);
                Ok(())
            }
        }
    }

    fn commit_batch_with_versions(&mut self, batch: &[BatchOp]) -> Result<Vec<u64>, DbError> {
        self.commit_batch_with_versions_for_telemetry(batch, 0)
    }

    fn commit_batch_with_versions_for_telemetry(
        &mut self,
        batch: &[BatchOp],
        queue_wait_ns: u64,
    ) -> Result<Vec<u64>, DbError> {
        self.commit_batch_with_versions_for_mode(
            batch,
            queue_wait_ns,
            ReplicationCommitMode::Quorum,
            None,
        )
    }

    fn commit_batch_with_versions_for_mode(
        &mut self,
        batch: &[BatchOp],
        queue_wait_ns: u64,
        replication_mode: ReplicationCommitMode,
        mesh: Option<&PrivateMeshContext>,
    ) -> Result<Vec<u64>, DbError> {
        let pp = preprocess_batch(batch)?;
        self.sync_keyrange_ownership_state()?;
        let route = self.route_batch_to_shard(batch)?;
        let ownership_fence = self.current_ownership_fence_for_route(&route)?;
        let mut result = self.prepare_and_apply_batch(
            batch,
            pp,
            queue_wait_ns,
            0,
            replication_mode,
            mesh,
            &ownership_fence,
        )?;
        self.submit_wal_and_record_stage(&mut result);
        // Direct engine path has no detached apply lane; complete apply visibility
        // inline once durability is established.
        self.mark_group_durable(result.active_group_id, result.required_index);
        if result.staged_ops.is_empty() {
            self.mark_group_apply_visible(result.active_group_id, result.required_index);
        } else {
            self.apply_staged_ops(&result.staged_ops);
            self.mark_group_apply_visible(result.active_group_id, result.required_index);
            result.staged_ops.clear();
        }
        result.deferred.execute();
        Ok(result.committed_versions)
    }

    fn prepare_batch_for_outside_replication(
        &mut self,
        batch: &[BatchOp],
        pp: PreProcessedBatch,
        queue_wait_ns: u64,
        engine_lock_wait_ns: u64,
        ownership_fence: &OwnershipFence,
    ) -> Result<PreparedOutsideLockBatch, DbError> {
        let total_started = Instant::now();
        let validate_started = Instant::now();
        for op in batch {
            match op {
                BatchOp::Put { namespace, .. } | BatchOp::Delete { namespace, .. } => {
                    self.authorize_write_namespace(namespace)?;
                }
            }
        }
        self.sync_keyrange_ownership_state()?;
        let route = self.route_batch_to_shard(batch)?;
        self.enforce_ownership_fence_for_route(&route, ownership_fence)?;
        let active_group_id = route.active_group_id;
        self.replication_telemetry
            .observe_batch(pp.op_count, pp.byte_count);
        self.evaluate_schema_gate_for_epoch(self.schema_committed_epoch)?;
        let validate_route_ns = duration_to_nanos(validate_started.elapsed());

        let (group_current_term, group_last_log_index, membership) = {
            let replication = self.replication_for_group(active_group_id)?;
            (
                replication.leader.current_term,
                replication.leader.last_log_index(),
                replication.membership.clone(),
            )
        };
        let required_term = group_current_term.max(self.raft_current_term);
        let required_index = group_last_log_index.saturating_add(pp.frame.command_count as u64);

        let mut shadow_versions: HashMap<EncodedUserKey, Option<u64>> =
            HashMap::with_capacity(batch.len());
        let mut staged_records = Vec::with_capacity(batch.len());
        let mut staged_ops = Vec::with_capacity(batch.len());
        let mut committed_versions = Vec::with_capacity(batch.len());
        let versions = self.tick_batch_clock(batch.len());
        let mut max_version = self.clock.peek().pack();

        for (op, &version) in batch.iter().zip(versions.iter()) {
            let (namespace, key, expected_version) = match op {
                BatchOp::Put {
                    namespace,
                    key,
                    expected_version,
                    ..
                } => (namespace, key, *expected_version),
                BatchOp::Delete {
                    namespace,
                    key,
                    expected_version,
                } => (namespace, key, *expected_version),
            };
            let user_key = encode_user_key_smallvec(namespace, key)?;
            let current = shadow_versions
                .get(&user_key)
                .copied()
                .unwrap_or_else(|| self.memtable.latest_version(user_key.as_slice()));
            validate_expected_version(expected_version, current)?;

            committed_versions.push(version);
            max_version = max_version.max(version);
            shadow_versions.insert(user_key.clone(), Some(version));

            match op {
                BatchOp::Put {
                    namespace,
                    key,
                    value,
                    ..
                } => {
                    staged_records.push(Record {
                        kind: RecordKind::Put,
                        namespace: namespace.clone(),
                        key: key.clone(),
                        value: value.clone(),
                        version,
                    });
                    staged_ops.push(StagedApplyOp::Put {
                        user_key: user_key.clone(),
                        namespace: namespace.clone(),
                        key: key.clone(),
                        value: value.clone(),
                        version,
                    });
                }
                BatchOp::Delete { namespace, key, .. } => {
                    staged_records.push(Record {
                        kind: RecordKind::Delete,
                        namespace: namespace.clone(),
                        key: key.clone(),
                        value: Bytes::new(),
                        version,
                    });
                    staged_ops.push(StagedApplyOp::Delete {
                        user_key: user_key.clone(),
                        namespace: namespace.clone(),
                        key: key.clone(),
                        version,
                    });
                }
            }
        }

        let staged_entries: Vec<LogEntry> = pp
            .command_payloads
            .into_iter()
            .enumerate()
            .map(|(offset, payload)| LogEntry {
                index: group_last_log_index
                    .saturating_add(offset as u64)
                    .saturating_add(1),
                term: required_term,
                payload,
            })
            .collect();

        self.append_entries_to_leader_log(active_group_id, &staged_entries)?;
        self.jupiter_telemetry.frontier_speculative_plans = self
            .jupiter_telemetry
            .frontier_speculative_plans
            .saturating_add(1);

        let mut voter_ids = membership.voters().clone();
        if let Some(joint) = membership.joint() {
            voter_ids.extend(joint.outgoing_voters.iter().copied());
            voter_ids.extend(joint.incoming_voters.iter().copied());
        }
        let mut replica_latency_rank = BTreeMap::new();
        for node_id in voter_ids {
            if node_id == LOCAL_NODE_ID {
                continue;
            }
            replica_latency_rank.insert(
                node_id,
                self.replication_telemetry.replica_priority_rank(node_id),
            );
        }
        let (leader_commit, leader_snapshot, follower_snapshots) = {
            let replication = self.replication_for_group(active_group_id)?;
            (
                replication.leader.commit_index,
                replication.leader.clone(),
                replication
                    .followers
                    .iter()
                    .map(|(node_id, state)| (*node_id, state.clone()))
                    .collect::<Vec<_>>(),
            )
        };
        let mut follower_progress_hints = BTreeMap::new();
        for (node_id, state) in &follower_snapshots {
            let follower_progress = state.last_log_index().max(state.commit_index);
            let prior_progress = self.sorted_run_progress.get(node_id).copied().unwrap_or(0);
            follower_progress_hints.insert(*node_id, follower_progress.max(prior_progress));
        }

        Ok(PreparedOutsideLockBatch {
            active_group_id,
            required_term,
            required_index,
            logical_shard_id: route.logical_shard_id,
            ownership_fence: ownership_fence.clone(),
            batch_ops: batch.to_vec(),
            membership,
            write_quorum_required: self.write_quorum as usize,
            replica_latency_rank,
            follower_progress_hints,
            leader_commit,
            leader_snapshot,
            follower_snapshots,
            require_private_rpc_transport: matches!(
                self.quorum_transport_mode,
                QuorumTransportMode::RequirePrivateRpc
            ),
            simulation_fallback_allowed: simulation_replication_fallback_allowed(),
            committed_versions,
            staged_records,
            staged_ops,
            staged_entries,
            max_version,
            op_count: pp.op_count as u64,
            byte_count: pp.byte_count as u64,
            queue_wait_ns,
            engine_lock_wait_ns,
            validate_route_ns,
            total_started,
        })
    }

    fn finalize_prepared_batch_after_outside_replication(
        &mut self,
        mut prepared: PreparedOutsideLockBatch,
        fanout_result: Result<OutsideLockFanoutResult, OutsideLockReplicationError>,
    ) -> Result<PrepareAndApplyResult, DbError> {
        let wave_avg_targets = |wave_count: usize, total_targets: usize| {
            if wave_count == 0 {
                0
            } else {
                total_targets / wave_count
            }
        };

        let fanout = match fanout_result {
            Ok(fanout) => {
                self.replication_telemetry
                    .set_queue_depth(fanout.total_target_count as u64);
                self.replication_telemetry.record_fanout_shape(
                    fanout.total_target_count,
                    fanout.contacted_target_count,
                    fanout.replication_wave_count,
                    wave_avg_targets(
                        fanout.replication_wave_count,
                        fanout.replication_wave_total_targets,
                    ),
                    fanout.replication_wave_max_targets,
                    fanout.successful_target_count,
                    fanout.failed_target_count,
                    fanout.cancelled_target_count,
                    fanout
                        .total_target_count
                        .saturating_sub(fanout.contacted_target_count),
                    fanout.aborted_in_flight_count,
                );
                self.jupiter_telemetry.frontier_wave_plans = self
                    .jupiter_telemetry
                    .frontier_wave_plans
                    .saturating_add(fanout.replication_wave_count as u64);
                fanout
            }
            Err(err) => {
                self.replication_telemetry
                    .set_queue_depth(err.total_target_count as u64);
                self.replication_telemetry
                    .record_failure(err.token, err.detail.clone(), false);
                self.replication_telemetry.record_fanout_shape(
                    err.total_target_count,
                    err.contacted_target_count,
                    err.replication_wave_count,
                    wave_avg_targets(
                        err.replication_wave_count,
                        err.replication_wave_total_targets,
                    ),
                    err.replication_wave_max_targets,
                    err.successful_target_count,
                    err.failed_target_count,
                    err.cancelled_target_count,
                    err.total_target_count
                        .saturating_sub(err.contacted_target_count),
                    err.aborted_in_flight_count,
                );
                self.replication_telemetry.clear_queue_depth();
                if let Some(replication_error) = err.replication_error {
                    self.replication_telemetry.increment_failure_counter(
                        replication_failure_token_for_message(&replication_error.message),
                    );
                    return Err(retryable_quorum_limit_error(
                        err.token,
                        format!(
                            "{}; follower_error={}",
                            err.detail, replication_error.message
                        ),
                    ));
                }
                return Err(retryable_quorum_limit_error(err.token, err.detail));
            }
        };
        if fanout.used_simulation {
            self.replication_telemetry.simulation_commits = self
                .replication_telemetry
                .simulation_commits
                .saturating_add(1);
        }
        if fanout.sorted_run_chunks_sent > 0 {
            self.jupiter_telemetry.sorted_run_catchup_chunks_sent = self
                .jupiter_telemetry
                .sorted_run_catchup_chunks_sent
                .saturating_add(fanout.sorted_run_chunks_sent);
        }
        if !fanout.follower_state_updates.is_empty() {
            let replication = self.replication_for_group_mut(prepared.active_group_id)?;
            for (node_id, state) in &fanout.follower_state_updates {
                replication.followers.insert(*node_id, state.clone());
            }
        }
        for (node_id, progress) in &fanout.follower_progress_updates {
            let entry = self.sorted_run_progress.entry(*node_id).or_insert(0);
            *entry = (*entry).max(*progress);
        }

        let ack_decision = evaluate_leader_ack(&LeaderAckInput {
            voters: prepared.membership.voters().len(),
            leader_durable: true,
            required_term: prepared.required_term,
            required_index: prepared.required_index,
            follower_responses: fanout.follower_responses.clone(),
        });
        self.replication_telemetry.record_ack_decision(
            ack_decision.durable_acks,
            ack_decision.quorum_size,
            ack_decision.quorum_replication_latency_ns,
            ack_decision.quorum_fsync_latency_ns,
            prepared.required_term,
            prepared.required_index,
            &fanout.follower_responses,
        );
        let mut durable_acks = BTreeSet::from([LOCAL_NODE_ID]);
        for follower in &fanout.follower_responses {
            if response_is_durable_ack(
                &follower.response,
                prepared.required_term,
                prepared.required_index,
            ) {
                durable_acks.insert(follower.node_id);
            }
        }
        if !ack_decision.ack_emitted
            || !prepared.membership.has_durable_quorum(&durable_acks)
            || durable_acks.len() < prepared.write_quorum_required
        {
            let detail = format!(
                "durability quorum not reached; durable_acks={} quorum={}",
                ack_decision.durable_acks, ack_decision.quorum_size
            );
            self.replication_telemetry.record_failure(
                QUORUM_FAILURE_TOKEN_DURABILITY_NOT_REACHED,
                detail.clone(),
                true,
            );
            self.replication_telemetry.clear_queue_depth();
            if let Some(err) = fanout.replication_error {
                self.replication_telemetry
                    .increment_failure_counter(replication_failure_token_for_message(&err.message));
                return Err(retryable_quorum_limit_error(
                    QUORUM_FAILURE_TOKEN_DURABILITY_NOT_REACHED,
                    format!("{detail}; follower_error={}", err.message),
                ));
            }
            return Err(retryable_quorum_limit_error(
                QUORUM_FAILURE_TOKEN_DURABILITY_NOT_REACHED,
                detail,
            ));
        }
        self.replication_telemetry.clear_queue_depth();
        #[cfg(test)]
        self.pending_append_responses.clear();

        let raft_persist_started = Instant::now();
        let (raft_meta_opt, deferred_topology) =
            self.maybe_raft_meta_for_wal(prepared.active_group_id, prepared.required_index);
        let mut deferred_raft_metadata = None;
        if let Some(meta) = raft_meta_opt {
            if matches!(
                self.replicated_log_backend,
                ReplicatedLogBackend::CanonicalOnly
            ) {
                deferred_raft_metadata = Some((self.wal_path.clone(), meta));
            } else {
                prepared.staged_records.push(record_from_raft_meta(
                    meta.current_term,
                    meta.voted_for,
                    meta.commit_index,
                    meta.needs_membership_flush,
                ));
            }
        }
        let raft_persist_ns = duration_to_nanos(raft_persist_started.elapsed());

        let wal_ops = prepared.staged_records.len();
        let payload_bytes = prepared
            .staged_entries
            .iter()
            .map(|entry| entry.payload.len())
            .sum::<usize>();
        let wal_len_for_telemetry: usize = prepared
            .staged_records
            .iter()
            .map(|record| {
                HEADER_BYTES + record.namespace.len() + record.key.len() + record.value.len()
            })
            .sum();
        let wal_records = prepared.staged_records;
        let wal_bytes = Vec::new();
        let encode_ns = 0;
        if matches!(
            self.replicated_log_backend,
            ReplicatedLogBackend::ShadowCanonical
        ) {
            self.replicated_log_telemetry
                .observe_batch(payload_bytes, wal_len_for_telemetry);
        }

        let shard_ops_delta = Some((prepared.logical_shard_id, prepared.batch_ops.len() as u64));
        let apply_started = Instant::now();
        if matches!(
            self.commit_visibility_mode,
            CommitVisibilityMode::StrictApply
        ) {
            self.apply_staged_ops(&prepared.staged_ops);
        }
        let decision = self.compaction_admission_decision_for_group(prepared.active_group_id);
        match decision {
            crate::db::lsm::scheduler::CompactionAdmissionDecision::Admit => {
                self.jupiter_telemetry.compaction_scheduler_admitted = self
                    .jupiter_telemetry
                    .compaction_scheduler_admitted
                    .saturating_add(1);
            }
            crate::db::lsm::scheduler::CompactionAdmissionDecision::Defer => {
                self.jupiter_telemetry.compaction_scheduler_deferred = self
                    .jupiter_telemetry
                    .compaction_scheduler_deferred
                    .saturating_add(1);
            }
            crate::db::lsm::scheduler::CompactionAdmissionDecision::Reject => {
                self.jupiter_telemetry.compaction_scheduler_rejected = self
                    .jupiter_telemetry
                    .compaction_scheduler_rejected
                    .saturating_add(1);
            }
        }
        // Memtable version-chain GC: prune stale MVCC versions below the
        // node's current safe-read watermark. Throttled by ops interval so it
        // doesn't run on every write group.
        self.memtable_gc_write_counter = self.memtable_gc_write_counter.saturating_add(1);
        if MEMTABLE_GC_ENABLED {
            let interval = MEMTABLE_GC_OPS_INTERVAL.max(1);
            if self.memtable_gc_write_counter % interval == 0 {
                let gc_watermark = self.watermarks.node_safe_read(LOCAL_NODE_ID).unwrap_or(0);
                if gc_watermark > 0 {
                    let gc_metrics = self.memtable.gc_old_versions(gc_watermark);
                    self.mark_lsm_stats_dirty();
                    self.jupiter_telemetry.memtable_gc_runs =
                        self.jupiter_telemetry.memtable_gc_runs.saturating_add(1);
                    self.jupiter_telemetry.memtable_gc_versions_dropped = self
                        .jupiter_telemetry
                        .memtable_gc_versions_dropped
                        .saturating_add(gc_metrics.versions_dropped);
                    self.jupiter_telemetry.memtable_gc_tombstone_keys_removed = self
                        .jupiter_telemetry
                        .memtable_gc_tombstone_keys_removed
                        .saturating_add(gc_metrics.tombstone_keys_removed);
                }
            }
        }
        let apply_ns = if matches!(
            self.commit_visibility_mode,
            CommitVisibilityMode::StrictApply
        ) {
            duration_to_nanos(apply_started.elapsed())
        } else {
            0
        };
        if matches!(
            self.commit_visibility_mode,
            CommitVisibilityMode::StrictApply
        ) {
            prepared.staged_ops.clear();
        }

        {
            let replication = self.replication_for_group_mut(prepared.active_group_id)?;
            replication.leader.commit_index = prepared.required_index;
        }
        let (raft_last_log_index, raft_last_committed_index, raft_current_term) = {
            let primary = self.primary_replication();
            (
                primary.leader.last_log_index(),
                primary.leader.commit_index,
                primary.leader.current_term,
            )
        };
        self.raft_last_log_index = raft_last_log_index;
        self.raft_last_committed_index = raft_last_committed_index;
        self.raft_current_term = raft_current_term;

        let clock_persist_started = Instant::now();
        let deferred_clock = self.maybe_capture_deferred_clock_persist(prepared.max_version);
        let clock_persist_ns = duration_to_nanos(clock_persist_started.elapsed());

        let deferred = DeferredPersistWork {
            raft_metadata: deferred_raft_metadata,
            clock_packed: deferred_clock,
            topology_state: deferred_topology,
        };
        let stage_data = WriteStagePartialData {
            op_count: prepared.op_count,
            byte_count: prepared.byte_count,
            queue_wait_ns: prepared.queue_wait_ns,
            engine_lock_wait_ns: prepared.engine_lock_wait_ns,
            validate_route_ns: prepared.validate_route_ns,
            replicate_ns: fanout.replicate_ns,
            apply_ns,
            raft_persist_ns,
            clock_persist_ns,
            total_started: prepared.total_started,
        };
        Ok(PrepareAndApplyResult {
            active_group_id: prepared.active_group_id,
            required_index: prepared.required_index,
            committed_versions: prepared.committed_versions,
            staged_ops: prepared.staged_ops,
            deferred,
            wal_records,
            wal_bytes,
            wal_ops,
            encode_ns,
            stage_data,
            shard_ops_delta,
        })
    }

    /// Prepare and apply a batch under the DbEngine lock. Encodes WAL records
    /// but does NOT sync to disk. Caller must call submit_wal_and_record_stage
    /// (or append_raw_bytes_with_metrics + record) after releasing the lock.
    fn prepare_and_apply_batch(
        &mut self,
        batch: &[BatchOp],
        pp: PreProcessedBatch,
        queue_wait_ns: u64,
        engine_lock_wait_ns: u64,
        replication_mode: ReplicationCommitMode,
        mesh: Option<&PrivateMeshContext>,
        ownership_fence: &OwnershipFence,
    ) -> Result<PrepareAndApplyResult, DbError> {
        let total_started = Instant::now();
        let validate_started = Instant::now();
        // Validation already done in preprocess_batch; only auth + routing
        // need engine state.
        for op in batch {
            match op {
                BatchOp::Put { namespace, .. } | BatchOp::Delete { namespace, .. } => {
                    self.authorize_write_namespace(namespace)?;
                }
            }
        }
        self.sync_keyrange_ownership_state()?;
        let route = self.route_batch_to_shard(batch)?;
        self.enforce_ownership_fence_for_route(&route, ownership_fence)?;
        let active_group_id = route.active_group_id;
        self.replication_telemetry
            .observe_batch(pp.op_count, pp.byte_count);
        self.evaluate_schema_gate_for_epoch(self.schema_committed_epoch)?;
        let validate_route_ns = duration_to_nanos(validate_started.elapsed());

        let (group_current_term, group_last_log_index) = {
            let replication = self.replication_for_group(active_group_id)?;
            (
                replication.leader.current_term,
                replication.leader.last_log_index(),
            )
        };
        let required_term = group_current_term.max(self.raft_current_term);
        let required_index = group_last_log_index.saturating_add(pp.frame.command_count as u64);

        let mut shadow_versions: HashMap<EncodedUserKey, Option<u64>> =
            HashMap::with_capacity(batch.len());
        let mut staged_records = Vec::with_capacity(batch.len());
        let mut staged_ops = Vec::with_capacity(batch.len());
        let mut committed_versions = Vec::with_capacity(batch.len());
        let versions = self.tick_batch_clock(batch.len());
        let mut max_version = self.clock.peek().pack();

        for (op, &version) in batch.iter().zip(versions.iter()) {
            let (namespace, key, expected_version) = match op {
                BatchOp::Put {
                    namespace,
                    key,
                    expected_version,
                    ..
                } => (namespace, key, *expected_version),
                BatchOp::Delete {
                    namespace,
                    key,
                    expected_version,
                } => (namespace, key, *expected_version),
            };
            let user_key = encode_user_key_smallvec(namespace, key)?;
            let current = shadow_versions
                .get(&user_key)
                .copied()
                .unwrap_or_else(|| self.memtable.latest_version(user_key.as_slice()));
            validate_expected_version(expected_version, current)?;

            committed_versions.push(version);
            max_version = max_version.max(version);
            shadow_versions.insert(user_key.clone(), Some(version));

            match op {
                BatchOp::Put {
                    namespace,
                    key,
                    value,
                    ..
                } => {
                    staged_records.push(Record {
                        kind: RecordKind::Put,
                        namespace: namespace.clone(),
                        key: key.clone(),
                        value: value.clone(),
                        version,
                    });
                    staged_ops.push(StagedApplyOp::Put {
                        user_key: user_key.clone(),
                        namespace: namespace.clone(),
                        key: key.clone(),
                        value: value.clone(),
                        version,
                    });
                }
                BatchOp::Delete { namespace, key, .. } => {
                    staged_records.push(Record {
                        kind: RecordKind::Delete,
                        namespace: namespace.clone(),
                        key: key.clone(),
                        value: Bytes::new(),
                        version,
                    });
                    staged_ops.push(StagedApplyOp::Delete {
                        user_key: user_key.clone(),
                        namespace: namespace.clone(),
                        key: key.clone(),
                        version,
                    });
                }
            }
        }

        // Use pre-computed command payloads instead of recomputing under lock.
        let staged_entries: Vec<LogEntry> = pp
            .command_payloads
            .into_iter()
            .enumerate()
            .map(|(offset, payload)| LogEntry {
                index: group_last_log_index
                    .saturating_add(offset as u64)
                    .saturating_add(1),
                term: required_term,
                payload,
            })
            .collect();

        let replicate_started = Instant::now();
        let replicate_ns = match replication_mode {
            ReplicationCommitMode::Quorum => {
                self.replicate_entries_for_quorum(
                    mesh,
                    active_group_id,
                    required_term,
                    required_index,
                    batch,
                    &staged_entries,
                    ownership_fence,
                )?;
                duration_to_nanos(replicate_started.elapsed())
            }
            ReplicationCommitMode::ReplicaLocal => {
                self.append_entries_to_leader_log(active_group_id, &staged_entries)?;
                0
            }
        };

        let raft_persist_started = Instant::now();
        let (raft_meta_opt, deferred_topology) =
            self.maybe_raft_meta_for_wal(active_group_id, required_index);
        let mut deferred_raft_metadata = None;
        if let Some(meta) = raft_meta_opt {
            if matches!(
                self.replicated_log_backend,
                ReplicatedLogBackend::CanonicalOnly
            ) {
                deferred_raft_metadata = Some((self.wal_path.clone(), meta));
            } else {
                staged_records.push(record_from_raft_meta(
                    meta.current_term,
                    meta.voted_for,
                    meta.commit_index,
                    meta.needs_membership_flush,
                ));
            }
        }
        let raft_persist_ns = duration_to_nanos(raft_persist_started.elapsed());

        let wal_ops = staged_records.len();
        let payload_bytes = staged_entries
            .iter()
            .map(|entry| entry.payload.len())
            .sum::<usize>();
        let wal_len_for_telemetry: usize = staged_records
            .iter()
            .map(|record| {
                HEADER_BYTES + record.namespace.len() + record.key.len() + record.value.len()
            })
            .sum();
        let wal_records = staged_records;
        let wal_bytes = Vec::new();
        let encode_ns = 0;
        if matches!(
            self.replicated_log_backend,
            ReplicatedLogBackend::ShadowCanonical
        ) {
            self.replicated_log_telemetry
                .observe_batch(payload_bytes, wal_len_for_telemetry);
        }

        let shard_ops_delta = Some((route.logical_shard_id, batch.len() as u64));
        let apply_started = Instant::now();
        if matches!(
            self.commit_visibility_mode,
            CommitVisibilityMode::StrictApply
        ) {
            self.apply_staged_ops(&staged_ops);
        }
        let decision = self.compaction_admission_decision_for_group(active_group_id);
        match decision {
            crate::db::lsm::scheduler::CompactionAdmissionDecision::Admit => {
                self.jupiter_telemetry.compaction_scheduler_admitted = self
                    .jupiter_telemetry
                    .compaction_scheduler_admitted
                    .saturating_add(1);
            }
            crate::db::lsm::scheduler::CompactionAdmissionDecision::Defer => {
                self.jupiter_telemetry.compaction_scheduler_deferred = self
                    .jupiter_telemetry
                    .compaction_scheduler_deferred
                    .saturating_add(1);
            }
            crate::db::lsm::scheduler::CompactionAdmissionDecision::Reject => {
                self.jupiter_telemetry.compaction_scheduler_rejected = self
                    .jupiter_telemetry
                    .compaction_scheduler_rejected
                    .saturating_add(1);
            }
        }
        let apply_ns = if matches!(
            self.commit_visibility_mode,
            CommitVisibilityMode::StrictApply
        ) {
            duration_to_nanos(apply_started.elapsed())
        } else {
            0
        };
        if matches!(
            self.commit_visibility_mode,
            CommitVisibilityMode::StrictApply
        ) {
            staged_ops.clear();
        }

        {
            let replication = self.replication_for_group_mut(active_group_id)?;
            replication.leader.commit_index = required_index;
        }
        let (raft_last_log_index, raft_last_committed_index, raft_current_term) = {
            let primary = self.primary_replication();
            (
                primary.leader.last_log_index(),
                primary.leader.commit_index,
                primary.leader.current_term,
            )
        };
        self.raft_last_log_index = raft_last_log_index;
        self.raft_last_committed_index = raft_last_committed_index;
        self.raft_current_term = raft_current_term;

        let clock_persist_started = Instant::now();
        let deferred_clock = self.maybe_capture_deferred_clock_persist(max_version);
        let clock_persist_ns = duration_to_nanos(clock_persist_started.elapsed());

        let deferred = DeferredPersistWork {
            raft_metadata: deferred_raft_metadata,
            clock_packed: deferred_clock,
            topology_state: deferred_topology,
        };
        let stage_data = WriteStagePartialData {
            op_count: pp.op_count as u64,
            byte_count: pp.byte_count as u64,
            queue_wait_ns,
            engine_lock_wait_ns,
            validate_route_ns,
            replicate_ns,
            apply_ns,
            raft_persist_ns,
            clock_persist_ns,
            total_started,
        };
        Ok(PrepareAndApplyResult {
            active_group_id,
            required_index,
            committed_versions,
            staged_ops,
            deferred,
            wal_records,
            wal_bytes,
            wal_ops,
            encode_ns,
            stage_data,
            shard_ops_delta,
        })
    }

    /// Submit WAL bytes (blocks on fsync) and record write-stage telemetry.
    /// Must be called after releasing the DbEngine lock.
    fn submit_wal_and_record_stage(&mut self, result: &mut PrepareAndApplyResult) {
        let append_started = Instant::now();
        let wal_ops = result.wal_ops;
        let (_, wal_metrics) = if result.wal_bytes.is_empty() && !result.wal_records.is_empty() {
            let (encode_ns, metrics_result) = encode_records_to_wal_bytes(
                &result.wal_records,
                |bytes, encode_ns| {
                    (
                        encode_ns,
                        self.wal.append_raw_bytes_with_metrics_slice(bytes, wal_ops, encode_ns),
                    )
                },
            );
            result.encode_ns = encode_ns;
            result.wal_records.clear();
            metrics_result
        } else {
            self.wal.append_raw_bytes_with_metrics(
                std::mem::take(&mut result.wal_bytes),
                result.wal_ops,
                result.encode_ns,
            )
        }
        .unwrap_or_else(|err| {
            panic!("FATAL: local WAL write failed after reaching Raft quorum; must crash to prevent split-brain state. IO Error: {}", err)
        });
        let wal_append_ns = result
            .encode_ns
            .saturating_add(duration_to_nanos(append_started.elapsed()));
        self.write_stage.record(DbWriteStageSample {
            op_count: result.stage_data.op_count,
            byte_count: result.stage_data.byte_count,
            queue_wait_ns: result.stage_data.queue_wait_ns,
            engine_lock_wait_ns: result.stage_data.engine_lock_wait_ns,
            validate_route_ns: result.stage_data.validate_route_ns,
            replicate_ns: result.stage_data.replicate_ns,
            wal_append_ns,
            wal_submit_wait_ns: wal_append_ns.saturating_sub(result.encode_ns),
            wal_hol_wait_ns: 0,
            wal_queue_wait_ns: wal_metrics.queue_wait_ns,
            wal_encode_ns: wal_metrics.encode_ns,
            wal_fdatasync_ns: wal_metrics.fdatasync_ns,
            wal_mutex_wait_ns: wal_metrics.mutex_wait_ns,
            apply_ns: result.stage_data.apply_ns,
            raft_persist_ns: result.stage_data.raft_persist_ns,
            clock_persist_ns: result.stage_data.clock_persist_ns,
            total_ns: duration_to_nanos(result.stage_data.total_started.elapsed()),
            lane_dequeue_to_complete_ns: duration_to_nanos(
                result.stage_data.total_started.elapsed(),
            ),
            queue_to_complete_ns: result
                .stage_data
                .queue_wait_ns
                .saturating_add(duration_to_nanos(result.stage_data.total_started.elapsed())),
        });
    }

    pub fn submit_batch(&mut self, batch: &[BatchOp]) -> Result<u64, DbError> {
        let versions = self.commit_batch_with_versions(batch)?;
        Ok(versions.iter().copied().max().unwrap_or(0))
    }

    pub fn submit_batch_replica_local_only(&mut self, batch: &[BatchOp]) -> Result<u64, DbError> {
        let versions = self.commit_batch_with_versions_for_mode(
            batch,
            0,
            ReplicationCommitMode::ReplicaLocal,
            None,
        )?;
        Ok(versions.iter().copied().max().unwrap_or(0))
    }

    fn refresh_replication_followers(&mut self) {
        for replication in self.replication_groups.values_mut() {
            let mut membership_nodes = replication.membership.voters().clone();
            membership_nodes.extend(replication.membership.learners().iter().copied());
            if let Some(joint) = replication.membership.joint() {
                membership_nodes.extend(joint.outgoing_voters.iter().copied());
                membership_nodes.extend(joint.incoming_voters.iter().copied());
                membership_nodes.extend(joint.outgoing_learners.iter().copied());
            }

            replication.followers.retain(|node_id, _| {
                *node_id != LOCAL_NODE_ID && membership_nodes.contains(node_id)
            });
            for node_id in membership_nodes {
                if node_id == LOCAL_NODE_ID {
                    continue;
                }
                replication
                    .followers
                    .entry(node_id)
                    .or_insert_with(|| NodeState::with_timing(node_id, 0, 10));
            }
        }
    }

    pub fn set_membership_voters(
        &mut self,
        voters: impl IntoIterator<Item = u64>,
    ) -> Result<(), DbError> {
        let new_membership = MembershipConfig::new(voters)
            .map_err(|err| DbError::invalid_argument(format!("membership invalid: {err:?}")))?;
        let previous_groups = self.replication_groups.clone();
        self.topology_state_dirty = true;
        for replication in self.replication_groups.values_mut() {
            replication.membership = new_membership.clone();
        }
        self.refresh_replication_followers();
        if let Err(err) = self.persist_raft_state_required() {
            self.replication_groups = previous_groups;
            return Err(err);
        }
        Ok(())
    }

    pub fn begin_membership_change(
        &mut self,
        change: MembershipChange,
        log_index: u64,
    ) -> Result<(), DbError> {
        let previous_groups = self.replication_groups.clone();
        self.topology_state_dirty = true;
        for replication in self.replication_groups.values_mut() {
            replication
                .membership
                .begin_joint_change(change.clone(), log_index)
                .map_err(|err| {
                    DbError::invalid_argument(format!("membership change rejected: {err:?}"))
                })?;
        }
        self.refresh_replication_followers();
        if let Err(err) = self.persist_raft_state_required() {
            self.replication_groups = previous_groups;
            return Err(err);
        }
        Ok(())
    }

    pub fn commit_membership_change(&mut self) -> Result<(), DbError> {
        let previous_groups = self.replication_groups.clone();
        self.topology_state_dirty = true;
        for replication in self.replication_groups.values_mut() {
            replication
                .membership
                .commit_joint_change()
                .map_err(|err| {
                    DbError::invalid_argument(format!("membership commit rejected: {err:?}"))
                })?;
        }
        self.refresh_replication_followers();
        if let Err(err) = self.persist_raft_state_required() {
            self.replication_groups = previous_groups;
            return Err(err);
        }
        Ok(())
    }

    pub fn abort_membership_change(&mut self) -> Result<(), DbError> {
        let previous_groups = self.replication_groups.clone();
        self.topology_state_dirty = true;
        for replication in self.replication_groups.values_mut() {
            replication.membership.abort_joint_change().map_err(|err| {
                DbError::invalid_argument(format!("membership abort rejected: {err:?}"))
            })?;
        }
        self.refresh_replication_followers();
        if let Err(err) = self.persist_raft_state_required() {
            self.replication_groups = previous_groups;
            return Err(err);
        }
        Ok(())
    }

    fn append_entries_to_leader_log(
        &mut self,
        active_group_id: u32,
        entries: &[LogEntry],
    ) -> Result<(), DbError> {
        let replication = self.replication_for_group_mut(active_group_id)?;
        for entry in entries {
            replication
                .leader
                .append_log_entry_checked(entry.clone())
                .map_err(|_| DbError::invalid_argument("non-contiguous leader log append"))?;
        }
        Ok(())
    }

    fn replicate_entries_for_quorum(
        &mut self,
        mesh: Option<&PrivateMeshContext>,
        active_group_id: u32,
        required_term: u64,
        required_index: u64,
        batch: &[BatchOp],
        entries: &[LogEntry],
        ownership_fence: &OwnershipFence,
    ) -> Result<(), DbError> {
        #[cfg(test)]
        let pending_responses = self.pending_append_responses.clone();
        #[cfg(test)]
        let use_pending = !pending_responses.is_empty();

        self.append_entries_to_leader_log(active_group_id, entries)?;
        self.jupiter_telemetry.frontier_speculative_plans = self
            .jupiter_telemetry
            .frontier_speculative_plans
            .saturating_add(1);

        let (membership, leader_commit) = {
            let replication = self.replication_for_group(active_group_id)?;
            (
                replication.membership.clone(),
                replication.leader.commit_index,
            )
        };
        let mut voter_ids = membership.voters().clone();
        if let Some(joint) = membership.joint() {
            voter_ids.extend(joint.outgoing_voters.iter().copied());
            voter_ids.extend(joint.incoming_voters.iter().copied());
        }
        let mut remote_voter_ids = voter_ids
            .into_iter()
            .filter(|node_id| *node_id != LOCAL_NODE_ID)
            .collect::<Vec<_>>();
        remote_voter_ids.sort_unstable();
        remote_voter_ids.dedup();
        // Keep identity mapping stable (node_id -> mesh node) even when fanout
        // priority ranking changes between commits.
        let remote_voter_ids_for_mapping = remote_voter_ids.clone();
        remote_voter_ids.sort_unstable_by_key(|node_id| {
            (
                self.replication_telemetry.replica_priority_rank(*node_id),
                *node_id,
            )
        });
        let total_target_count = remote_voter_ids.len();
        let mut contacted_target_count = 0usize;
        let mut replication_wave_count = 0usize;
        let mut replication_wave_total_targets = 0usize;
        let mut replication_wave_max_targets = 0usize;
        let mut successful_target_count = 0usize;
        let mut failed_target_count = 0usize;
        let mut cancelled_target_count = 0usize;
        let mut aborted_in_flight_count = 0usize;
        let wave_avg_targets = |wave_count: usize, total_targets: usize| {
            if wave_count == 0 {
                0
            } else {
                total_targets / wave_count
            }
        };
        let replication_max_in_flight = REPLICATION_MAX_IN_FLIGHT;
        let replication_max_targets = REPLICATION_MAX_TARGETS;
        if remote_voter_ids.len() > replication_max_targets {
            let detail = format!(
                "replication target count {} exceeds max targets {}",
                remote_voter_ids.len(),
                replication_max_targets
            );
            self.replication_telemetry.record_failure(
                QUORUM_FAILURE_TOKEN_REPLICATION_IN_FLIGHT_LIMIT,
                detail.clone(),
                false,
            );
            self.replication_telemetry.record_fanout_shape(
                total_target_count,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                total_target_count,
                0,
            );
            self.replication_telemetry.clear_queue_depth();
            return Err(retryable_quorum_limit_error(
                QUORUM_FAILURE_TOKEN_REPLICATION_IN_FLIGHT_LIMIT,
                detail,
            ));
        }
        self.replication_telemetry
            .set_queue_depth(remote_voter_ids.len() as u64);
        self.replication_telemetry.record_fanout_shape(
            total_target_count,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            total_target_count,
            0,
        );

        let mut follower_responses = Vec::new();
        let mut replication_error: Option<DbError> = None;
        let write_quorum_required = self.write_quorum as usize;
        let quorum_satisfied = |acks: &BTreeSet<u64>| {
            membership.has_durable_quorum(acks) && acks.len() >= write_quorum_required
        };
        let mut provisional_durable_acks = BTreeSet::from([LOCAL_NODE_ID]);
        #[cfg(test)]
        if use_pending {
            follower_responses = pending_responses;
        }
        #[cfg(not(test))]
        let use_pending = false;

        if !use_pending {
            let mesh_leader = mesh.filter(|mesh| mesh.is_leader());
            if matches!(
                self.quorum_transport_mode,
                QuorumTransportMode::RequirePrivateRpc
            ) && !remote_voter_ids.is_empty()
                && mesh_leader.is_none()
            {
                let detail = format!(
                    "quorum transport requires private rpc but mesh leader path unavailable; voters_required={}",
                    remote_voter_ids.len()
                );
                self.replication_telemetry.record_failure(
                    QUORUM_FAILURE_TOKEN_PRIVATE_RPC_REQUIRED,
                    detail.clone(),
                    false,
                );
                self.replication_telemetry.record_fanout_shape(
                    total_target_count,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    total_target_count,
                    0,
                );
                self.replication_telemetry.clear_queue_depth();
                return Err(retryable_quorum_limit_error(
                    QUORUM_FAILURE_TOKEN_PRIVATE_RPC_REQUIRED,
                    detail,
                ));
            }
            if let Some(mesh) = mesh_leader {
                if let Err(err) = mesh.ensure_ready_for("replication quorum write") {
                    let detail = format!("private mesh readiness check failed: {}", err.message);
                    self.replication_telemetry.record_failure(
                        QUORUM_FAILURE_TOKEN_PRIVATE_MESH_NOT_READY,
                        detail.clone(),
                        false,
                    );
                    self.replication_telemetry.record_fanout_shape(
                        total_target_count,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        total_target_count,
                        0,
                    );
                    self.replication_telemetry.clear_queue_depth();
                    return Err(retryable_quorum_limit_error(
                        QUORUM_FAILURE_TOKEN_PRIVATE_MESH_NOT_READY,
                        detail,
                    ));
                }
                let node_mapping = map_remote_voters_to_mesh_nodes(
                    &remote_voter_ids_for_mapping,
                    mesh.follower_nodes(),
                );
                if node_mapping.len() < remote_voter_ids.len() {
                    let detail = format!(
                        "replication quorum target set too small: followers={} voters_required={}",
                        node_mapping.len(),
                        remote_voter_ids.len()
                    );
                    self.replication_telemetry.record_failure(
                        QUORUM_FAILURE_TOKEN_TARGET_SET_TOO_SMALL,
                        detail.clone(),
                        false,
                    );
                    self.replication_telemetry.record_fanout_shape(
                        total_target_count,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        total_target_count,
                        0,
                    );
                    self.replication_telemetry.clear_queue_depth();
                    return Err(retryable_quorum_limit_error(
                        QUORUM_FAILURE_TOKEN_TARGET_SET_TOO_SMALL,
                        detail,
                    ));
                }
                let mut replication_targets = Vec::with_capacity(remote_voter_ids.len());
                for (node_id, node_name) in node_mapping {
                    let Some(address) = mesh.address_for_node(&node_name) else {
                        if replication_error.is_none() {
                            replication_error = Some(DbError::limit(format!(
                                "private mesh follower address missing for {node_name}; RETRY_AFTER_MS=25"
                            )));
                        }
                        continue;
                    };
                    replication_targets.push((node_id, address));
                }
                replication_targets.sort_unstable_by_key(|(node_id, _)| {
                    (
                        self.replication_telemetry.replica_priority_rank(*node_id),
                        *node_id,
                    )
                });

                let request_template = crate::db::rpc::grpc::WriteBatchRequest {
                    handle: 0,
                    ops: batch.to_vec(),
                    idempotency_token: None,
                    expected_home_epoch: ownership_fence.expected_home_epoch,
                    expected_shard_map_epoch: ownership_fence.expected_shard_map_epoch,
                    ownership_token: ownership_fence.ownership_token.clone(),
                };
                let proto_request_template =
                    crate::db::rpc::tonic_service::write_batch_request_to_proto(request_template);
                let io_timeout = mesh.io_timeout;
                let total_targets = replication_targets.len();
                let mut hedge_extra = REPLICATION_HEDGE_EXTRA;
                if hedge_extra > 0 && REPLICATION_DYNAMIC_HEDGE {
                    let rpc_snapshot =
                        crate::db::rpc::private_network::replication_rpc_in_flight_snapshot();
                    if rpc_snapshot.available_permits.saturating_mul(2) < rpc_snapshot.max_in_flight
                    {
                        hedge_extra = 0;
                    }
                }
                let mut wave_start = 0usize;
                while wave_start < total_targets {
                    let remaining_targets = total_targets.saturating_sub(wave_start);
                    let additional_needed = additional_acks_needed_for_quorum(
                        &membership,
                        &provisional_durable_acks,
                        write_quorum_required,
                    )
                    .max(1);
                    let wave_size = additional_needed
                        .saturating_add(hedge_extra)
                        .min(replication_max_in_flight.max(1))
                        .min(remaining_targets.max(1));
                    self.jupiter_telemetry.frontier_wave_plans =
                        self.jupiter_telemetry.frontier_wave_plans.saturating_add(1);
                    replication_wave_count = replication_wave_count.saturating_add(1);
                    replication_wave_total_targets =
                        replication_wave_total_targets.saturating_add(wave_size);
                    replication_wave_max_targets = replication_wave_max_targets.max(wave_size);
                    let wave_end = (wave_start + wave_size).min(total_targets);
                    self.replication_telemetry
                        .set_queue_depth((total_targets - wave_start) as u64);
                    let (fanout_results, wave_aborted_count) = if wave_size == 1 {
                        let (node_id, address) = replication_targets[wave_start].clone();
                        let token = format!(
                            "mesh-quorum-{active_group_id}-{required_term}-{required_index}-{node_id}"
                        );
                        let mut proto_request = proto_request_template.clone();
                        proto_request.idempotency_token = Some(token);
                        let result = block_on_runtime(async move {
                            let replicate_started = Instant::now();
                            let response = crate::db::rpc::private_network::replicate_write_batch_proto_prefer_stream_async(
                                &address,
                                proto_request,
                                io_timeout,
                            )
                            .await;
                            (
                                node_id,
                                duration_to_nanos(replicate_started.elapsed()).max(1),
                                response,
                            )
                        });
                        (vec![Ok(result)], 0usize)
                    } else {
                        block_on_runtime(async {
                            let mut join_set = tokio::task::JoinSet::new();
                            for idx in wave_start..wave_end {
                                let (node_id, address) = replication_targets[idx].clone();
                                let token = format!(
                                    "mesh-quorum-{active_group_id}-{required_term}-{required_index}-{node_id}"
                                );
                                let mut proto_request = proto_request_template.clone();
                                proto_request.idempotency_token = Some(token);
                                join_set.spawn(async move {
                                    let replicate_started = Instant::now();
                                    let response = crate::db::rpc::private_network::replicate_write_batch_proto_prefer_stream_async(
                                        &address,
                                        proto_request,
                                        io_timeout,
                                    )
                                    .await;
                                    (
                                        node_id,
                                        duration_to_nanos(replicate_started.elapsed()).max(1),
                                        response,
                                    )
                                });
                            }

                            let mut joined = Vec::new();
                            let mut successful_acks = 0usize;
                            let mut aborted_count = 0usize;
                            while let Some(result) = join_set.join_next().await {
                                let mut reached_wave_quorum = false;
                                if let Ok((_node_id, _latency_ns, Ok(_))) = &result {
                                    successful_acks = successful_acks.saturating_add(1);
                                    if successful_acks >= additional_needed {
                                        reached_wave_quorum = true;
                                    }
                                }
                                joined.push(result);
                                if reached_wave_quorum {
                                    if !join_set.is_empty() {
                                        aborted_count =
                                            aborted_count.saturating_add(join_set.len());
                                        join_set.abort_all();
                                    }
                                    break;
                                }
                            }
                            (joined, aborted_count)
                        })
                    };
                    aborted_in_flight_count =
                        aborted_in_flight_count.saturating_add(wave_aborted_count);

                    for result in fanout_results {
                        match result {
                            Ok((node_id, latency_ns, Ok(_))) => {
                                contacted_target_count = contacted_target_count.saturating_add(1);
                                successful_target_count = successful_target_count.saturating_add(1);
                                provisional_durable_acks.insert(node_id);
                                follower_responses.push(FollowerAppendResponse {
                                    node_id,
                                    response: AppendEntriesResponse {
                                        term: required_term,
                                        success: true,
                                        match_index: required_index,
                                        conflict_index: None,
                                    },
                                    replication_latency_ns: latency_ns,
                                    fsync_latency_ns: latency_ns,
                                });
                            }
                            Ok((_node_id, _latency_ns, Err(err))) => {
                                contacted_target_count = contacted_target_count.saturating_add(1);
                                failed_target_count = failed_target_count.saturating_add(1);
                                let mapped = map_private_rpc_error(err);
                                self.replication_telemetry.increment_failure_counter(
                                    replication_failure_token_for_message(&mapped.message),
                                );
                                if replication_error.is_none() {
                                    replication_error = Some(mapped);
                                }
                            }
                            Err(err) => {
                                if err.is_cancelled() {
                                    cancelled_target_count =
                                        cancelled_target_count.saturating_add(1);
                                    continue;
                                }
                                failed_target_count = failed_target_count.saturating_add(1);
                                self.replication_telemetry.increment_failure_counter(
                                    QUORUM_FAILURE_TOKEN_PRIVATE_RPC_TASK_JOIN,
                                );
                                if replication_error.is_none() {
                                    replication_error = Some(DbError::limit(format!(
                                        "private mesh replication task join failed: {err}; RETRY_AFTER_MS=25"
                                    )));
                                }
                            }
                        }
                    }
                    self.replication_telemetry.record_fanout_shape(
                        total_target_count,
                        contacted_target_count,
                        replication_wave_count,
                        wave_avg_targets(replication_wave_count, replication_wave_total_targets),
                        replication_wave_max_targets,
                        successful_target_count,
                        failed_target_count,
                        cancelled_target_count,
                        total_target_count.saturating_sub(contacted_target_count),
                        aborted_in_flight_count,
                    );
                    if quorum_satisfied(&provisional_durable_acks) {
                        break;
                    }
                    wave_start = wave_end;
                }
            } else {
                if !remote_voter_ids.is_empty() && !simulation_replication_fallback_allowed() {
                    let detail = format!(
                        "replication fallback to local simulation disabled; voters_required={}",
                        remote_voter_ids.len()
                    );
                    self.replication_telemetry.record_failure(
                        QUORUM_FAILURE_TOKEN_SIMULATION_DISABLED,
                        detail.clone(),
                        false,
                    );
                    self.replication_telemetry.record_fanout_shape(
                        total_target_count,
                        contacted_target_count,
                        replication_wave_count,
                        wave_avg_targets(replication_wave_count, replication_wave_total_targets),
                        replication_wave_max_targets,
                        successful_target_count,
                        failed_target_count,
                        cancelled_target_count,
                        total_target_count.saturating_sub(contacted_target_count),
                        aborted_in_flight_count,
                    );
                    self.replication_telemetry.clear_queue_depth();
                    return Err(retryable_quorum_limit_error(
                        QUORUM_FAILURE_TOKEN_SIMULATION_DISABLED,
                        detail,
                    ));
                }
                self.replication_telemetry.simulation_commits = self
                    .replication_telemetry
                    .simulation_commits
                    .saturating_add(1);
                let leader_snapshot = self.replication_for_group(active_group_id)?.leader.clone();
                for node_id in remote_voter_ids.iter().copied() {
                    replication_wave_count = replication_wave_count.saturating_add(1);
                    replication_wave_total_targets =
                        replication_wave_total_targets.saturating_add(1);
                    replication_wave_max_targets = replication_wave_max_targets.max(1);
                    contacted_target_count = contacted_target_count.saturating_add(1);
                    let follower_state = self
                        .replication_for_group_mut(active_group_id)?
                        .followers
                        .entry(node_id)
                        .or_insert_with(|| NodeState::with_timing(node_id, 0, 10));
                    let replicate_started = Instant::now();
                    match replicate_to_follower(&leader_snapshot, follower_state, leader_commit) {
                        Ok(response) => {
                            let replication_latency_ns =
                                duration_to_nanos(replicate_started.elapsed()).max(1);
                            if response_is_durable_ack(&response, required_term, required_index) {
                                provisional_durable_acks.insert(node_id);
                            }
                            if response.success {
                                successful_target_count = successful_target_count.saturating_add(1);
                            } else {
                                failed_target_count = failed_target_count.saturating_add(1);
                            }
                            follower_responses.push(FollowerAppendResponse {
                                node_id,
                                response,
                                replication_latency_ns,
                                // Local simulation does not model separate follower fsync yet;
                                // record wall time to avoid hiding this component.
                                fsync_latency_ns: replication_latency_ns,
                            });
                        }
                        Err(err) => {
                            failed_target_count = failed_target_count.saturating_add(1);
                            if replication_error.is_none() {
                                replication_error = Some(err);
                            }
                        }
                    }
                    self.replication_telemetry.record_fanout_shape(
                        total_target_count,
                        contacted_target_count,
                        replication_wave_count,
                        wave_avg_targets(replication_wave_count, replication_wave_total_targets),
                        replication_wave_max_targets,
                        successful_target_count,
                        failed_target_count,
                        cancelled_target_count,
                        total_target_count.saturating_sub(contacted_target_count),
                        aborted_in_flight_count,
                    );
                    if quorum_satisfied(&provisional_durable_acks) {
                        break;
                    }
                }
            }
        }

        if use_pending && contacted_target_count == 0 {
            contacted_target_count = follower_responses.len();
            if contacted_target_count > 0 {
                replication_wave_count = 1;
                replication_wave_total_targets = contacted_target_count;
                replication_wave_max_targets = contacted_target_count;
            }
            successful_target_count = follower_responses
                .iter()
                .filter(|follower| {
                    response_is_durable_ack(&follower.response, required_term, required_index)
                })
                .count();
            failed_target_count = contacted_target_count.saturating_sub(successful_target_count);
            self.replication_telemetry.record_fanout_shape(
                total_target_count,
                contacted_target_count,
                replication_wave_count,
                wave_avg_targets(replication_wave_count, replication_wave_total_targets),
                replication_wave_max_targets,
                successful_target_count,
                failed_target_count,
                cancelled_target_count,
                total_target_count.saturating_sub(contacted_target_count),
                aborted_in_flight_count,
            );
        }

        let ack_decision = evaluate_leader_ack(&LeaderAckInput {
            voters: membership.voters().len(),
            leader_durable: true,
            required_term,
            required_index,
            follower_responses: follower_responses.clone(),
        });
        self.replication_telemetry.record_ack_decision(
            ack_decision.durable_acks,
            ack_decision.quorum_size,
            ack_decision.quorum_replication_latency_ns,
            ack_decision.quorum_fsync_latency_ns,
            required_term,
            required_index,
            &follower_responses,
        );
        let mut durable_acks = BTreeSet::from([LOCAL_NODE_ID]);
        for follower in &follower_responses {
            if response_is_durable_ack(&follower.response, required_term, required_index) {
                durable_acks.insert(follower.node_id);
            }
        }
        if !ack_decision.ack_emitted
            || !membership.has_durable_quorum(&durable_acks)
            || durable_acks.len() < self.write_quorum as usize
        {
            let detail = format!(
                "durability quorum not reached; durable_acks={} quorum={}",
                ack_decision.durable_acks, ack_decision.quorum_size
            );
            self.replication_telemetry.record_failure(
                QUORUM_FAILURE_TOKEN_DURABILITY_NOT_REACHED,
                detail.clone(),
                true,
            );
            self.replication_telemetry.clear_queue_depth();
            if let Some(err) = replication_error {
                self.replication_telemetry
                    .increment_failure_counter(replication_failure_token_for_message(&err.message));
                let detail = format!("{detail}; follower_error={}", err.message);
                return Err(retryable_quorum_limit_error(
                    QUORUM_FAILURE_TOKEN_DURABILITY_NOT_REACHED,
                    detail,
                ));
            }
            return Err(retryable_quorum_limit_error(
                QUORUM_FAILURE_TOKEN_DURABILITY_NOT_REACHED,
                detail,
            ));
        }
        self.replication_telemetry.clear_queue_depth();
        #[cfg(test)]
        self.pending_append_responses.clear();
        Ok(())
    }

    fn apply_staged_ops(&mut self, staged_ops: &[StagedApplyOp]) {
        let mut max_applied_version = None;
        // Pre-compute namespace strings once per unique namespace to avoid
        // String::from_utf8_lossy().to_string() per op.
        let mut ns_cache: HashMap<*const [u8], String> = HashMap::new();
        let ns_string = |cache: &mut HashMap<*const [u8], String>, ns: &[u8]| -> String {
            cache
                .entry(ns as *const [u8])
                .or_insert_with(|| String::from_utf8_lossy(ns).into_owned())
                .clone()
        };
        for op in staged_ops {
            match op {
                StagedApplyOp::Put {
                    user_key,
                    namespace,
                    key,
                    value,
                    version,
                } => {
                    let (stored_value, externalized) =
                        externalize_value_for_memtable(&mut self.blob_store, value.clone());
                    self.memtable.apply(user_key, *version, Some(stored_value));
                    if externalized {
                        self.jupiter_telemetry.blob_values_externalized = self
                            .jupiter_telemetry
                            .blob_values_externalized
                            .saturating_add(1);
                    }
                    self.cdc
                        .emit_put(namespace.clone(), key.clone(), value.clone(), *version);
                    self.read_path.observe_present_key(user_key);
                    let ns_str = ns_string(&mut ns_cache, namespace);
                    self.safe_time.observe_shard_safe_time_no_recompute(
                        ns_str,
                        LOCAL_REGION_ID,
                        *version,
                    );
                    max_applied_version = Some(max_applied_version.unwrap_or(0).max(*version));
                }
                StagedApplyOp::Delete {
                    user_key,
                    namespace,
                    key,
                    version,
                } => {
                    self.memtable.apply(user_key, *version, None);
                    self.cdc
                        .emit_delete(namespace.clone(), key.clone(), *version);
                    self.read_path.observe_absent_key(user_key);
                    let ns_str = ns_string(&mut ns_cache, namespace);
                    self.safe_time.observe_shard_safe_time_no_recompute(
                        ns_str,
                        LOCAL_REGION_ID,
                        *version,
                    );
                    max_applied_version = Some(max_applied_version.unwrap_or(0).max(*version));
                }
            }
        }
        if let Some(version) = max_applied_version {
            self.safe_time
                .observe_shard_safe_time_no_recompute("clock", LOCAL_REGION_ID, version);
        }
        // Single recomputation after all updates instead of per-op.
        if !staged_ops.is_empty() {
            self.safe_time.recompute_region_safe_times();
            self.mark_lsm_stats_dirty();
            self.maybe_run_blob_gc_cycle();
        }
    }

    fn read_version_for_consistency(
        &self,
        consistency: ReadConsistency,
        requested_ts: Option<u64>,
    ) -> Result<u64, DbError> {
        match consistency {
            ReadConsistency::Eventual => Ok(self
                .watermarks
                .node_safe_read(LOCAL_NODE_ID)
                .unwrap_or_else(|| self.clock.peek().pack())),
            ReadConsistency::Strong => {
                if matches!(
                    self.commit_visibility_mode,
                    CommitVisibilityMode::AsyncApply
                ) {
                    let status = self.commit_visibility_status();
                    if status.apply_backlog_depth > 0 {
                        return Err(DbError::limit(format!(
                            "STRONG_READ_APPLY_BACKLOG: apply_visible_index={} durability_commit_index={}; RETRY_AFTER_MS=25",
                            status.apply_visible_index, status.durability_commit_index
                        )));
                    }
                }
                let node_safe = self.watermarks.node_safe_read(LOCAL_NODE_ID);
                let propagated_safe = self.safe_time.global_safe_time();
                let safe_time = match (propagated_safe, node_safe) {
                    (Some(propagated), Some(node)) => Some(propagated.max(node)),
                    (Some(propagated), None) => Some(propagated),
                    (None, Some(node)) => Some(node),
                    (None, None) => None,
                };
                let requested = requested_ts
                    .unwrap_or_else(|| safe_time.unwrap_or_else(|| self.clock.peek().pack()));
                let safe_time = safe_time.unwrap_or(requested);
                let uncertainty = self.uncertainty.window_for_read_packed(requested);
                enforce_strong_read(requested, safe_time, uncertainty).map_err(|err| {
                    let token = match err.code {
                        StrongReadErrorCode::SafeTimeLag => "STRONG_READ_SAFE_TIME_LAG",
                        StrongReadErrorCode::UncertaintyWindow => "STRONG_READ_UNCERTAINTY_WINDOW",
                    };
                    DbError::limit(format!("{token}: {}", err.explain))
                })?;
                Ok(requested)
            }
        }
    }

    pub fn read_point(
        &self,
        namespace: &[u8],
        key: &[u8],
        consistency: ReadConsistency,
        requested_ts: Option<u64>,
    ) -> Result<Option<Vec<u8>>, DbError> {
        let read_version = self.read_version_for_consistency(consistency, requested_ts)?;
        let user_key = encode_user_key(namespace, key)?;
        let shortcut_policy = if requested_ts.is_some() {
            PointShortcutPolicy::KeyOnlyShortcutsDisabled
        } else {
            PointShortcutPolicy::KeyOnlyShortcutsEnabled
        };
        self.read_path.read_point(&user_key, shortcut_policy, || {
            self.memtable
                .visible(&user_key, read_version)
                .and_then(|value| materialize_value_from_memtable(&self.blob_store, value))
        })
    }

    /// Like `read_point` but returns the version of the visible value.
    /// Used for idempotency lookups where the version is the commit_version.
    pub fn read_point_with_version(
        &self,
        namespace: &[u8],
        key: &[u8],
        consistency: ReadConsistency,
        requested_ts: Option<u64>,
    ) -> Result<Option<(u64, Vec<u8>)>, DbError> {
        let read_version = self.read_version_for_consistency(consistency, requested_ts)?;
        let user_key = encode_user_key(namespace, key)?;
        let shortcut_policy = if requested_ts.is_some() {
            PointShortcutPolicy::KeyOnlyShortcutsDisabled
        } else {
            PointShortcutPolicy::KeyOnlyShortcutsEnabled
        };
        self.read_path
            .read_point_with_version(&user_key, shortcut_policy, || {
                self.memtable
                    .visible_with_version(&user_key, read_version)
                    .and_then(|(ver, raw)| {
                        materialize_value_from_memtable(&self.blob_store, &raw).map(|v| (ver, v))
                    })
            })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn read_range_iter(
        &self,
        namespace: &[u8],
        start_key: &[u8],
        end_key: &[u8],
        limit: usize,
        cancellation: RangeCancellation,
        consistency: ReadConsistency,
        requested_ts: Option<u64>,
    ) -> Result<RangeIterator, DbError> {
        let read_version = self.read_version_for_consistency(consistency, requested_ts)?;
        let start = encode_user_key(namespace, start_key)?;
        let end = encode_user_key(namespace, end_key)?;
        let raw_rows = self
            .memtable
            .range_visible(&start, &end, read_version, limit);
        let mut rows = Vec::with_capacity(raw_rows.len());
        for (key, value, version) in raw_rows {
            let materialized = materialize_value_from_memtable(&self.blob_store, &value)
                .ok_or_else(|| DbError::io("blob reference missing during range read"))?;
            rows.push((key, Bytes::from(materialized), version));
        }
        self.read_path.begin_range(rows, cancellation)
    }

    pub fn read_range(
        &self,
        namespace: &[u8],
        start_key: &[u8],
        end_key: &[u8],
        limit: usize,
        consistency: ReadConsistency,
        requested_ts: Option<u64>,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>, u64)>, DbError> {
        let mut iter = self.read_range_iter(
            namespace,
            start_key,
            end_key,
            limit,
            RangeCancellation::new(),
            consistency,
            requested_ts,
        )?;
        let mut rows = Vec::new();
        while let Some(row) = iter.try_next()? {
            let (_namespace, user_key) = decode_user_key(&row.0)?;
            rows.push((user_key, row.1.to_vec(), row.2));
        }
        Ok(rows)
    }

    pub fn read_stats(&self) -> ReadPathStats {
        self.read_path.stats()
    }

    pub fn txn_begin(&mut self) -> Result<u64, DbError> {
        let txn_id = self.next_txn_id;
        self.next_txn_id = self.next_txn_id.saturating_add(1);
        let start_ts = self.tick_clock();
        self.txns.insert(
            txn_id,
            TxnRecord {
                state: TxnState::Active,
                start_ts,
                prepared_ts: None,
                commit_ts: None,
                bound_shard: None,
            },
        );
        Ok(txn_id)
    }

    pub fn txn_prepare(&mut self, txn_id: u64) -> Result<(), DbError> {
        let (state, start_ts) = self
            .txns
            .get(&txn_id)
            .map(|r| (r.state, r.start_ts))
            .ok_or_else(|| DbError::invalid_argument("unknown txn id"))?;
        match state {
            TxnState::Active => {
                let prepared_ts = self.tick_clock().max(start_ts);
                let record = self
                    .txns
                    .get_mut(&txn_id)
                    .ok_or_else(|| DbError::invalid_argument("unknown txn id"))?;
                record.state = TxnState::Prepared;
                record.prepared_ts = Some(prepared_ts);
                Ok(())
            }
            TxnState::Prepared => Ok(()),
            TxnState::Committed => Err(DbError::invalid_argument(
                "txn already committed and cannot be prepared",
            )),
            TxnState::Aborted => Err(DbError::invalid_argument(
                "txn already aborted and cannot be prepared",
            )),
        }
    }

    pub fn txn_commit(&mut self, txn_id: u64) -> Result<(), DbError> {
        let (state, lower_bound) = self
            .txns
            .get(&txn_id)
            .map(|r| (r.state, r.prepared_ts.unwrap_or(r.start_ts)))
            .ok_or_else(|| DbError::invalid_argument("unknown txn id"))?;
        match state {
            TxnState::Active | TxnState::Prepared => {
                let commit_ts = self.tick_clock().max(lower_bound);
                let record = self
                    .txns
                    .get_mut(&txn_id)
                    .ok_or_else(|| DbError::invalid_argument("unknown txn id"))?;
                record.state = TxnState::Committed;
                record.commit_ts = Some(commit_ts);
                self.lock_table.release_txn(txn_id);
                Ok(())
            }
            TxnState::Committed => Ok(()),
            TxnState::Aborted => Err(DbError::invalid_argument(
                "txn already aborted and cannot commit",
            )),
        }
    }

    fn txn_pending_work_units(&self, txn_id: u64) -> Result<(usize, u32), DbError> {
        let txn = self
            .txns
            .get(&txn_id)
            .ok_or_else(|| DbError::invalid_argument("unknown txn id"))?;
        let bound_shard = txn.bound_shard.unwrap_or(0);
        let lock_count = self
            .lock_table
            .snapshot()
            .held_locks
            .iter()
            .filter(|lock| lock.txn_id == txn_id)
            .count();
        Ok((lock_count.max(1), bound_shard))
    }

    pub fn txn_abort(&mut self, txn_id: u64) -> Result<(), DbError> {
        let record = self
            .txns
            .get_mut(&txn_id)
            .ok_or_else(|| DbError::invalid_argument("unknown txn id"))?;
        match record.state {
            TxnState::Active | TxnState::Prepared => {
                record.state = TxnState::Aborted;
                self.lock_table.release_txn(txn_id);
                Ok(())
            }
            TxnState::Committed => Err(DbError::invalid_argument(
                "txn already committed and cannot abort",
            )),
            TxnState::Aborted => Ok(()),
        }
    }

    fn snapshot_restore_single_node_guard(&self) -> Result<(), DbError> {
        if self.replication_factor != 1 || self.write_quorum != 1 {
            return Err(DbError::invalid_argument(format!(
                "SNAPSHOT_RESTORE_SINGLE_NODE_ONLY: replication_factor={} write_quorum={}",
                self.replication_factor, self.write_quorum
            )));
        }
        for (group_id, replication) in &self.replication_groups {
            let voter_count = replication.membership.voters().len();
            let joint_active = replication.membership.joint().is_some();
            if voter_count != 1 || joint_active {
                return Err(DbError::invalid_argument(format!(
                    "SNAPSHOT_RESTORE_SINGLE_NODE_ONLY: group_id={group_id} voters={voter_count} joint_active={joint_active}"
                )));
            }
        }
        Ok(())
    }

    fn reopen_config_for_restore(&self) -> DbConfig {
        use crate::db::config::{EngineConfig, ReplicationConfig, TopologyConfig};
        let mut checkpoint = self.checkpoint_config.clone();
        checkpoint.allowed_regions = self.checkpoint_allowed_regions.clone();
        DbConfig {
            replication: ReplicationConfig {
                factor: self.replication_factor,
                write_quorum: self.write_quorum,
                async_failover: self.replication_async_failover,
                commit_visibility_mode: self.commit_visibility_mode,
                log_backend: self.replicated_log_backend,
                quorum_transport_mode: self.quorum_transport_mode,
            },
            topology: TopologyConfig {
                initial_logical_shards: self.shard_directory.logical_shard_count(),
                initial_active_groups: self.shard_directory.active_group_count(),
                autoscale_enabled: self.autoscale_enabled,
                autoscale_tick_ms: self.autoscale_tick_ms,
                autoscale_max_skew_ratio: self.autoscale_max_skew_ratio,
                autoscale_target_shards_per_group: self.autoscale_target_shards_per_group,
                autoscale_max_active_groups: self.autoscale_max_active_groups,
                autoscale_max_logical_shards: self.autoscale_max_logical_shards,
                local_region: self.local_region.clone(),
                region_az_node_map: self.topology_region_az_node_map.clone(),
                residency_policy: self.residency_policy.clone(),
            },
            sovereignty: crate::db::config::SovereigntyConfig {
                id: self.sovereignty_id.clone(),
                allowed_regions: self.sovereignty_allowed_regions.clone(),
                enforce_all_copies: self.sovereignty_enforce_all_copies,
            },
            intent: self.intent_config.clone(),
            checkpoint,
            engine: EngineConfig {
                writer_lane_count: self.lane_wals.len().max(1),
                ..EngineConfig::default()
            },
            rpc: Default::default(),
            restore_latest_checkpoint_on_open: false,
            residency_policy: self.residency_policy.clone(),
        }
    }

    pub fn snapshot_start(&mut self) -> Result<u64, DbError> {
        let checkpoint = self.checkpoint_create()?;
        let snapshot_id = self.next_snapshot_id;
        self.next_snapshot_id = self.next_snapshot_id.saturating_add(1);
        let created_ts = self.tick_clock();
        self.snapshots.insert(
            snapshot_id,
            SnapshotRecord {
                created_ts,
                progress: 100,
                restored_ts: None,
                checkpoint_id: checkpoint.checkpoint_id,
            },
        );
        Ok(snapshot_id)
    }

    pub fn snapshot_status(&self, snapshot_id: u64) -> Result<u8, DbError> {
        self.snapshots
            .get(&snapshot_id)
            .map(|record| record.progress)
            .ok_or_else(|| DbError::invalid_argument("unknown snapshot id"))
    }

    pub fn restore_snapshot(&mut self, snapshot_id: u64) -> Result<(), DbError> {
        let (created_ts, checkpoint_id) = self
            .snapshots
            .get(&snapshot_id)
            .map(|r| (r.created_ts, r.checkpoint_id.clone()))
            .ok_or_else(|| DbError::invalid_argument("unknown snapshot id"))?;
        self.snapshot_restore_single_node_guard()?;
        self.checkpoint_restore_by_id(&checkpoint_id)?;

        let reopen_config = self.reopen_config_for_restore();
        let mut reopened = DbEngine::open_with_config(&self.wal_path, &reopen_config)?;
        let restored_ts = reopened.tick_clock().max(created_ts);
        let mut snapshots = self.snapshots.clone();
        let record = snapshots
            .get_mut(&snapshot_id)
            .ok_or_else(|| DbError::invalid_argument("unknown snapshot id"))?;
        record.restored_ts = Some(restored_ts);
        reopened.next_snapshot_id = self.next_snapshot_id;
        reopened.snapshots = snapshots;
        *self = reopened;
        Ok(())
    }

    fn force_abort_txn(&mut self, txn_id: u64) {
        if let Some(record) = self.txns.get_mut(&txn_id)
            && record.state != TxnState::Committed
        {
            record.state = TxnState::Aborted;
        }
        self.lock_table.release_txn(txn_id);
    }

    fn txn_lock_encoded(&mut self, txn_id: u64, encoded_key: Vec<u8>) -> Result<(), DbError> {
        let txn_state = self
            .txns
            .get(&txn_id)
            .map(|r| r.state)
            .ok_or_else(|| DbError::invalid_argument("unknown txn id"))?;
        if txn_state != TxnState::Active {
            return Err(DbError::invalid_argument(
                "txn must be active to acquire key locks",
            ));
        }

        let now = self.clock.peek().pack();
        match self.lock_table.acquire(txn_id, encoded_key.clone(), now) {
            LockAcquireOutcome::Acquired | LockAcquireOutcome::AlreadyHeld => Ok(()),
            LockAcquireOutcome::Waiting {
                holder_txn_id,
                victim_txn_id,
            } => {
                if let Some(victim_txn_id) = victim_txn_id {
                    self.force_abort_txn(victim_txn_id);
                    if victim_txn_id == txn_id {
                        return Err(DbError::limit(format!(
                            "deadlock victim txn={txn_id}; lock request aborted"
                        )));
                    }
                    return match self.lock_table.acquire(txn_id, encoded_key, now) {
                        LockAcquireOutcome::Acquired | LockAcquireOutcome::AlreadyHeld => Ok(()),
                        LockAcquireOutcome::Waiting { holder_txn_id, .. } => Err(DbError::limit(
                            format!("lock held by txn={holder_txn_id}; RETRY_AFTER_MS=25"),
                        )),
                    };
                }
                Err(DbError::limit(format!(
                    "lock held by txn={holder_txn_id}; RETRY_AFTER_MS=25"
                )))
            }
        }
    }

    pub fn txn_lock_key(
        &mut self,
        txn_id: u64,
        namespace: &[u8],
        key: &[u8],
    ) -> Result<(), DbError> {
        self.authorize_write_namespace(namespace)?;
        let route = self.route_key_to_shard(namespace, key)?;
        self.bind_txn_to_shard(txn_id, route.logical_shard_id)?;
        let encoded_key = encode_user_key(namespace, key)?;
        self.txn_lock_encoded(txn_id, encoded_key)
    }

    pub fn txn_lock_range(
        &mut self,
        txn_id: u64,
        namespace: &[u8],
        start_key: &[u8],
        end_key: &[u8],
    ) -> Result<(), DbError> {
        if end_key <= start_key {
            return Err(DbError::invalid_argument(
                "range end must be strictly greater than range start",
            ));
        }
        self.authorize_write_namespace(namespace)?;
        let start_route = self.route_key_to_shard(namespace, start_key)?;
        let end_route = self.route_key_to_shard(namespace, end_key)?;
        if start_route.logical_shard_id != end_route.logical_shard_id {
            return Err(DbError::cross_shard_txn(format!(
                "CROSS_SHARD_TXN_UNSUPPORTED: txn range spans shards {} and {}",
                start_route.logical_shard_id, end_route.logical_shard_id
            )));
        }
        self.bind_txn_to_shard(txn_id, start_route.logical_shard_id)?;

        let txn_state = self
            .txns
            .get(&txn_id)
            .map(|r| r.state)
            .ok_or_else(|| DbError::invalid_argument("unknown txn id"))?;
        if txn_state != TxnState::Active {
            return Err(DbError::invalid_argument(
                "txn must be active to acquire key locks",
            ));
        }

        let start = encode_user_key(namespace, start_key)?;
        let end = encode_user_key(namespace, end_key)?;
        let now = self.clock.peek().pack();
        match self
            .lock_table
            .acquire_range(txn_id, start.clone(), end.clone(), now)
        {
            LockAcquireOutcome::Acquired | LockAcquireOutcome::AlreadyHeld => Ok(()),
            LockAcquireOutcome::Waiting {
                holder_txn_id,
                victim_txn_id,
            } => {
                if let Some(victim_txn_id) = victim_txn_id {
                    self.force_abort_txn(victim_txn_id);
                    if victim_txn_id == txn_id {
                        return Err(DbError::limit(format!(
                            "deadlock victim txn={txn_id}; lock request aborted"
                        )));
                    }
                    return match self.lock_table.acquire_range(txn_id, start, end, now) {
                        LockAcquireOutcome::Acquired | LockAcquireOutcome::AlreadyHeld => Ok(()),
                        LockAcquireOutcome::Waiting { holder_txn_id, .. } => Err(DbError::limit(
                            format!("lock held by txn={holder_txn_id}; RETRY_AFTER_MS=25"),
                        )),
                    };
                }
                Err(DbError::limit(format!(
                    "lock held by txn={holder_txn_id}; RETRY_AFTER_MS=25"
                )))
            }
        }
    }

    pub fn uncertainty_window(&self) -> UncertaintyWindow {
        self.uncertainty
            .window_for_read_packed(self.clock.peek().pack())
    }

    pub fn flush_clock_state(&mut self) -> Result<(), DbError> {
        let packed = self.clock.peek().pack();
        self.persist_clock_state(packed)?;
        self.clock_persist_ops_since_flush = 0;
        self.clock_persist_error = None;
        self.clock_persist_error_at = None;
        Ok(())
    }

    pub fn flush_durable_state(&mut self) -> Result<(), DbError> {
        self.wal
            .force_flush_on_close()
            .map_err(|err| DbError::io(format!("wal close flush failed: {err}")))?;
        self.persist_raft_state_required()?;
        self.flush_clock_state()
    }

    #[cfg(test)]
    fn txn_record(&self, txn_id: u64) -> Option<TxnRecord> {
        self.txns.get(&txn_id).copied()
    }

    #[cfg(test)]
    fn clock_packed(&self) -> u64 {
        self.clock.peek().pack()
    }

    #[cfg(test)]
    fn lock_table_snapshot(&self) -> LockTableSnapshot {
        self.lock_table.snapshot()
    }

    #[cfg(test)]
    fn cdc_events(&self, after_commit_seq: u64, limit: usize) -> Vec<crate::db::cdc::CdcEvent> {
        self.cdc.events_since(after_commit_seq, limit)
    }

    fn cdc_page(
        &self,
        after_commit_seq: u64,
        limit: usize,
        shard_filter: Option<&[u8]>,
    ) -> crate::db::cdc::CdcPage {
        self.cdc.page_since(after_commit_seq, limit, shard_filter)
    }

    fn cdc_ack(&mut self, stream: &str, commit_seq: u64) -> Result<u64, DbError> {
        let mut staged = self.cdc_checkpoints.clone();
        let checkpoint = staged.ack(stream, commit_seq);
        let persist_result = persist_cdc_checkpoints(
            &self.wal_path,
            &staged,
            #[cfg(test)]
            self.fail_next_cdc_checkpoint_persist,
        );
        #[cfg(test)]
        {
            self.fail_next_cdc_checkpoint_persist = false;
        }
        if let Err(err) = persist_result {
            self.cdc_checkpoint_persist_error = Some(err.message.clone());
            self.cdc_checkpoint_persist_error_at = Some(now_epoch_s());
            return Err(err);
        }
        self.cdc_checkpoints = staged;
        self.cdc_checkpoint_persist_error = None;
        self.cdc_checkpoint_persist_error_at = None;
        Ok(checkpoint)
    }

    fn cdc_checkpoint(&self, stream: &str) -> Option<u64> {
        self.cdc_checkpoints.checkpoint(stream)
    }

    pub fn safe_time_diagnostics(&self, budgets: SafeTimeLagBudget) -> SafeTimeDiagnostics {
        self.safe_time
            .diagnostics(self.clock.peek().pack(), budgets)
    }

    fn logical_shard_count(&self) -> u32 {
        self.shard_directory.logical_shard_count()
    }

    fn active_group_count(&self) -> u32 {
        self.shard_directory.active_group_count()
    }

    fn shard_map_epoch(&self) -> u64 {
        self.shard_directory.epoch()
    }

    fn route_namespace_key(&self, namespace: &[u8], key: &[u8]) -> Result<u32, DbError> {
        Ok(self.route_key_to_shard(namespace, key)?.logical_shard_id)
    }

    fn split_logical_shard(&mut self, shard_id: u32) -> Result<(u32, u32), DbError> {
        let result = self
            .shard_directory
            .split_shard(shard_id)
            .map_err(|err| DbError::invalid_argument(format!("split shard failed: {err:?}")))?;
        self.sync_keyrange_ownership_state()?;
        self.persist_topology_state_required()?;
        Ok(result)
    }

    fn merge_logical_shards(&mut self, left: u32, right: u32) -> Result<u32, DbError> {
        let merged = self
            .shard_directory
            .merge_shards(left, right)
            .map_err(|err| DbError::invalid_argument(format!("merge shard failed: {err:?}")))?;
        self.sync_keyrange_ownership_state()?;
        self.persist_topology_state_required()?;
        Ok(merged)
    }

    fn resolve_owner(&mut self, namespace: &[u8], key: &[u8]) -> Result<OwnerRecord, DbError> {
        self.sync_keyrange_ownership_state()?;
        let route = self.route_key_to_shard(namespace, key)?;
        self.owner_record_for_route(&route)
    }

    fn plan_home_relocation(
        &mut self,
        keyrange_id: &str,
        target_region: &str,
        reason: &str,
    ) -> Result<crate::db::placement::RelocationJob, DbError> {
        self.sync_keyrange_ownership_state()?;
        let keyrange_id = keyrange_id.trim();
        let target_region = target_region.trim().to_ascii_lowercase();
        if keyrange_id.is_empty() || target_region.is_empty() {
            return Err(DbError::invalid_argument(
                "REHOME_PLAN_INVALID: keyrange_id and target_region are required",
            ));
        }
        let Some(owner) = self.keyrange_ownership.get(keyrange_id).cloned() else {
            return Err(DbError::invalid_argument(format!(
                "REHOME_PLAN_KEYRANGE_MISSING: keyrange={keyrange_id}"
            )));
        };
        let policy = self.ownership_locality_policy();
        if self.home_store.get_home(keyrange_id).is_none() {
            self.home_store
                .set_home(keyrange_id, &owner.home_region, &policy)
                .map_err(|err| {
                    DbError::invalid_argument(format!("REHOME_SET_HOME_FAILED: {err:?}"))
                })?;
        }
        self.home_store
            .relocate_home(keyrange_id, &target_region, reason, &policy)
            .map_err(|err| DbError::invalid_argument(format!("REHOME_PLAN_FAILED: {err:?}")))
    }

    fn advance_home_relocation(
        &mut self,
        job_id: &str,
        phase_ack: Option<crate::db::placement::RelocationPhase>,
    ) -> Result<crate::db::placement::RelocationJob, DbError> {
        self.sync_keyrange_ownership_state()?;
        let current = self
            .home_store
            .get_relocation(job_id)
            .map_err(|err| DbError::invalid_argument(format!("REHOME_JOB_MISSING: {err:?}")))?;
        if let Some(expected) = phase_ack
            && current.phase != expected
        {
            return Err(DbError::invalid_argument(format!(
                "REHOME_PHASE_ACK_MISMATCH: expected={expected:?} actual={:?}",
                current.phase
            )));
        }
        let next = self
            .home_store
            .advance_relocation(job_id)
            .map_err(|err| DbError::invalid_argument(format!("REHOME_ADVANCE_FAILED: {err:?}")))?;
        if matches!(next.phase, crate::db::placement::RelocationPhase::Finalize) {
            let shard_map_epoch = self.shard_directory.epoch();
            if let Some(owner) = self.keyrange_ownership.get_mut(&next.keyrange) {
                owner.home_region = next.target_home.clone();
                owner.home_epoch = owner.home_epoch.saturating_add(1);
                owner.ownership_token = Self::ownership_token_for(
                    &owner.keyrange_id,
                    &owner.sovereignty_id,
                    &owner.home_region,
                    owner.home_epoch,
                    shard_map_epoch,
                    &owner.leader_node_id,
                );
            }
        }
        self.persist_topology_state_required()?;
        Ok(next)
    }

    fn promote_async_failover(
        &mut self,
        keyrange_id: &str,
        region: &str,
        expected_epoch: u64,
    ) -> Result<OwnerRecord, DbError> {
        self.sync_keyrange_ownership_state()?;
        let keyrange_id = keyrange_id.trim();
        let region = region.trim().to_ascii_lowercase();
        let shard_map_epoch = self.shard_directory.epoch();
        {
            let owner = self.keyrange_ownership.get(keyrange_id).ok_or_else(|| {
                DbError::invalid_argument(format!(
                    "ASYNC_PROMOTION_KEYRANGE_MISSING: keyrange={keyrange_id}"
                ))
            })?;
            if owner.home_epoch != expected_epoch {
                return Err(DbError::invalid_argument(format!(
                    "ASYNC_PROMOTION_EPOCH_MISMATCH: expected={} actual={}",
                    expected_epoch, owner.home_epoch
                )));
            }
            if !owner.async_failover_regions.contains(&region) {
                return Err(DbError::invalid_argument(format!(
                    "ASYNC_PROMOTION_REGION_NOT_ALLOWED: keyrange={} region={}",
                    keyrange_id, region
                )));
            }
        }
        let policy = self.ownership_locality_policy();
        self.home_store
            .set_home(keyrange_id, &region, &policy)
            .map_err(|err| {
                DbError::invalid_argument(format!("ASYNC_PROMOTION_SET_HOME_FAILED: {err:?}"))
            })?;
        let owner_record = {
            let owner = self
                .keyrange_ownership
                .get_mut(keyrange_id)
                .ok_or_else(|| {
                    DbError::invalid_argument(format!(
                        "ASYNC_PROMOTION_KEYRANGE_MISSING: keyrange={keyrange_id}"
                    ))
                })?;
            owner.home_region = region;
            owner.home_epoch = owner.home_epoch.saturating_add(1);
            owner.ownership_token = Self::ownership_token_for(
                &owner.keyrange_id,
                &owner.sovereignty_id,
                &owner.home_region,
                owner.home_epoch,
                shard_map_epoch,
                &owner.leader_node_id,
            );
            OwnerRecord {
                keyrange_id: owner.keyrange_id.clone(),
                sovereignty_id: owner.sovereignty_id.clone(),
                home_region: owner.home_region.clone(),
                home_epoch: owner.home_epoch,
                leader_node_id: owner.leader_node_id.clone(),
                ownership_token: owner.ownership_token.clone(),
                shard_map_epoch,
                async_failover_regions: owner.async_failover_regions.iter().cloned().collect(),
            }
        };
        self.persist_topology_state_required()?;
        Ok(owner_record)
    }

    fn topology_status(&self) -> DbTopologyStatus {
        let mut groups = self
            .replication_groups
            .iter()
            .map(|(group_id, replication)| {
                let mut voters = replication
                    .membership
                    .voters()
                    .iter()
                    .copied()
                    .collect::<Vec<_>>();
                voters.sort_unstable();
                let mut learners = replication
                    .membership
                    .learners()
                    .iter()
                    .copied()
                    .collect::<Vec<_>>();
                learners.sort_unstable();
                DbGroupTopologyStatus {
                    group_id: *group_id,
                    voters,
                    learners,
                    current_term: replication.leader.current_term,
                    last_log_index: replication.leader.last_log_index(),
                    commit_index: replication.leader.commit_index,
                    durability_commit_index: replication.durability_commit_index,
                    apply_visible_index: replication.apply_visible_index,
                    apply_backlog_depth: replication
                        .durability_commit_index
                        .saturating_sub(replication.apply_visible_index),
                }
            })
            .collect::<Vec<_>>();
        groups.sort_by_key(|group| group.group_id);
        DbTopologyStatus {
            logical_shards: self.shard_directory.logical_shard_count(),
            active_groups: self.shard_directory.active_group_count(),
            shard_map_epoch: self.shard_directory.epoch(),
            replication_factor: self.replication_factor,
            write_quorum: self.write_quorum,
            groups,
        }
    }

    fn autoscale_status(&self) -> DbAutoscaleStatus {
        self.autoscale_status.clone()
    }

    fn record_autoscale_status(
        &mut self,
        last_action: impl Into<String>,
        reasons: Vec<String>,
        action_time_ms: u64,
    ) {
        self.autoscale_status.last_action = last_action.into();
        self.autoscale_status.reasons = reasons;
        self.autoscale_status.last_action_at_epoch_ms = action_time_ms;
    }

    fn hot_meta_guardrail_peak_write_ops(&self) -> u64 {
        let mut peak = self.shard_write_ops.values().copied().max().unwrap_or(0);
        if let Ok(accum) = self.shard_write_ops_accum.try_lock() {
            peak = peak.max(accum.values().copied().max().unwrap_or(0));
        }
        peak
    }

    fn tiering_policy_boundaries(&self) -> (u64, u64) {
        // Hook point for a future explicit tiering policy engine.
        (
            AUTOPILOT_TIERING_MIN_LIVE_BYTES,
            AUTOPILOT_TIERING_MAX_LIVE_BYTES,
        )
    }

    fn run_autopilot_controller_tick(&mut self, source: &str) {
        self.autopilot_action_seq = self.autopilot_action_seq.saturating_add(1);
        let (tier_min, tier_max) = self.tiering_policy_boundaries();
        let output = crate::db::autopilot::orchestrator::execute_controller_tick(
            crate::db::autopilot::orchestrator::ControllerInput {
                action_id: self.autopilot_action_seq,
                source: source.to_string(),
                now_epoch_ms: now_epoch_ms(),
                replication_factor: self.replication_factor,
                write_quorum: self.write_quorum,
                autoscale_enabled: self.autoscale_enabled,
                active_groups: self.shard_directory.active_group_count(),
                logical_shards: self.shard_directory.logical_shard_count(),
                observed_live_bytes: self.lsm_cached_stats.live_bytes_estimate,
                hot_meta_write_ops: self.hot_meta_guardrail_peak_write_ops(),
                hot_meta_max_write_ops: AUTOPILOT_HOTMETA_MAX_WRITE_OPS_PER_TICK,
                tiering_boundary_min_live_bytes: tier_min,
                tiering_boundary_max_live_bytes: tier_max,
            },
        );
        self.autopilot_intent_effective = output.intent_effective;
        self.autopilot_intent_conflicts = output.intent_conflicts;
        self.autopilot_tiering_state = output.tiering_state;
        self.autopilot_recommendations = output.recommendations;
        self.autopilot_audit_ring.push(output.audit_row);
    }

    fn intent_effective(&self) -> DbIntentEffective {
        self.autopilot_intent_effective.clone()
    }

    fn intent_conflicts(&self) -> Vec<DbIntentConflict> {
        self.autopilot_intent_conflicts.clone()
    }

    fn autopilot_last_actions(&self, limit: usize) -> Vec<DbAutopilotAuditRow> {
        self.autopilot_audit_ring.recent(limit)
    }

    fn tiering_state(&self) -> DbTieringState {
        self.autopilot_tiering_state.clone()
    }

    fn recommendations(&self) -> Vec<DbRecommendation> {
        self.autopilot_recommendations.clone()
    }

    fn discover_healthy_nodes(&self, allow_remote_discovery: bool) -> Vec<u64> {
        #[cfg(test)]
        if let Some(nodes) = &self.autoscale_test_healthy_nodes {
            return normalize_healthy_nodes(nodes.clone());
        }

        if let Some(nodes) = healthy_nodes_from_count_env() {
            return normalize_healthy_nodes(nodes);
        }
        if let Some(nodes) = healthy_nodes_from_names_env() {
            return normalize_healthy_nodes(nodes);
        }
        if allow_remote_discovery
            && let Some(nodes) = healthy_nodes_from_fly_api(&self.local_region)
        {
            return normalize_healthy_nodes(nodes);
        }
        let fallback = self
            .primary_replication()
            .membership
            .voters()
            .iter()
            .copied()
            .collect::<Vec<_>>();
        normalize_healthy_nodes(fallback)
    }

    fn desired_membership_for_group(
        &self,
        group_id: u32,
        healthy_nodes: &[u64],
    ) -> Result<MembershipConfig, DbError> {
        let nodes = normalize_healthy_nodes(healthy_nodes.to_vec());
        let replica_count = nodes
            .len()
            .min(self.replication_factor.max(1) as usize)
            .max(1);
        let mut voters = Vec::with_capacity(replica_count);
        let start = group_id as usize % nodes.len();
        for offset in 0..nodes.len() {
            let node = nodes[(start + offset) % nodes.len()];
            if !voters.contains(&node) {
                voters.push(node);
            }
            if voters.len() >= replica_count {
                break;
            }
        }
        MembershipConfig::new(voters).map_err(|err| {
            DbError::invalid_argument(format!(
                "autoscale membership synthesis rejected for group {group_id}: {err:?}"
            ))
        })
    }

    fn reconcile_replica_set_once(
        &mut self,
        healthy_nodes: &[u64],
        now_ms: u64,
    ) -> Result<bool, DbError> {
        let mut group_ids = self.replication_groups.keys().copied().collect::<Vec<_>>();
        group_ids.sort_unstable();
        for group_id in group_ids {
            let desired = self.desired_membership_for_group(group_id, healthy_nodes)?;
            let current = self.replication_for_group(group_id)?;
            let mismatch = current.membership.voters() != desired.voters()
                || !current.membership.learners().is_empty()
                || current.membership.joint().is_some();
            if !mismatch {
                continue;
            }

            let previous_membership = current.membership.clone();
            {
                let replication = self.replication_for_group_mut(group_id)?;
                replication.membership = desired.clone();
            }
            self.refresh_replication_followers();
            if let Err(err) = self.persist_raft_state_required() {
                if let Ok(replication) = self.replication_for_group_mut(group_id) {
                    replication.membership = previous_membership;
                }
                self.refresh_replication_followers();
                return Err(err);
            }

            self.record_autoscale_status(
                "update_replica_set",
                vec![format!(
                    "group {} voters updated to {:?}",
                    group_id,
                    desired.voters()
                )],
                now_ms,
            );
            return Ok(true);
        }
        Ok(false)
    }

    fn run_autoscale_tick(
        &mut self,
        force: bool,
        healthy_nodes_override: Option<Vec<u64>>,
    ) -> Result<DbAutoscaleStatus, DbError> {
        let now_ms = now_epoch_ms();
        let mode = self.autoscale_mode;
        if !self.autoscale_enabled {
            self.record_autoscale_status(
                "disabled",
                vec!["autoscale disabled".to_string()],
                now_ms,
            );
            return Ok(self.autoscale_status());
        }
        if !force
            && now_ms.saturating_sub(self.autoscale_last_tick_epoch_ms) < self.autoscale_tick_ms
        {
            self.record_autoscale_status(
                "cooldown",
                vec!["autoscale cooldown in effect".to_string()],
                self.autoscale_status.last_action_at_epoch_ms,
            );
            return Ok(self.autoscale_status());
        }
        self.autoscale_last_tick_epoch_ms = now_ms;
        self.autoscale_status.mode = mode;
        if let Ok(mut accum) = self.shard_write_ops_accum.try_lock() {
            for (shard_id, count) in accum.drain() {
                let c = self.shard_write_ops.entry(shard_id).or_insert(0);
                *c = c.saturating_add(count);
            }
        }

        if !matches!(mode, AutoscaleMode::GrowOnly) {
            self.record_autoscale_status(
                "hold",
                vec![format!("unsupported autoscale mode: {:?}", mode)],
                now_ms,
            );
            return Ok(self.autoscale_status());
        }

        if self
            .replication_groups
            .values()
            .any(|replication| replication.membership.joint().is_some())
        {
            self.record_autoscale_status(
                "hold",
                vec!["membership joint-change in progress".to_string()],
                now_ms,
            );
            return Ok(self.autoscale_status());
        }

        // Background autoscale ticks run under the engine lock; keep discovery local there.
        let healthy_nodes = healthy_nodes_override
            .map(normalize_healthy_nodes)
            .unwrap_or_else(|| self.discover_healthy_nodes(force));
        let healthy_node_count = healthy_nodes.len() as u32;
        if healthy_node_count < self.write_quorum {
            self.record_autoscale_status(
                "blocked",
                vec![format!(
                    "quorum simulation failed: healthy_nodes={} write_quorum={}",
                    healthy_node_count, self.write_quorum
                )],
                now_ms,
            );
            return Ok(self.autoscale_status());
        }

        if self.reconcile_replica_set_once(&healthy_nodes, now_ms)? {
            return Ok(self.autoscale_status());
        }

        let desired_groups = healthy_node_count.min(self.autoscale_max_active_groups.max(1));
        let current_groups = self.shard_directory.active_group_count();

        if current_groups < desired_groups {
            let new_group_id = self.shard_directory.add_active_group();
            let membership = self.desired_membership_for_group(new_group_id, &healthy_nodes)?;
            let template = self.primary_replication().clone();
            self.replication_groups.insert(
                new_group_id,
                ReplicationState {
                    leader: template.leader,
                    followers: HashMap::new(),
                    membership,
                    durability_commit_index: template.durability_commit_index,
                    apply_visible_index: template.apply_visible_index,
                },
            );
            self.refresh_replication_followers();
            self.persist_topology_state_required()?;
            self.record_autoscale_status(
                "grow_group",
                vec![format!(
                    "active groups increased from {} to {} (healthy_nodes={})",
                    current_groups,
                    self.shard_directory.active_group_count(),
                    healthy_node_count
                )],
                now_ms,
            );
            return Ok(self.autoscale_status());
        }

        if self.shard_write_ops.is_empty() {
            self.record_autoscale_status(
                "hold",
                vec!["no shard write telemetry".to_string()],
                now_ms,
            );
            return Ok(self.autoscale_status());
        }

        let total_ops = self.shard_write_ops.values().copied().sum::<u64>().max(1);
        let shard_sample_count = self.shard_directory.logical_shard_count().max(1) as f64;
        let mean_ops = total_ops as f64 / shard_sample_count;
        let (hottest_shard, hottest_ops) = self
            .shard_write_ops
            .iter()
            .max_by_key(|(_, ops)| **ops)
            .map(|(shard_id, ops)| (*shard_id, *ops))
            .unwrap_or((0, 0));
        let hottest_ratio = if mean_ops <= 0.0 {
            0.0
        } else {
            hottest_ops as f64 / mean_ops
        };
        let shard_density = self.shard_directory.logical_shard_count() as f64
            / self.shard_directory.active_group_count().max(1) as f64;
        let density_over_target = shard_density > self.autoscale_target_shards_per_group as f64;
        let split_needed = (hottest_ratio > self.autoscale_max_skew_ratio || density_over_target)
            && self.shard_directory.logical_shard_count() < self.autoscale_max_logical_shards;

        if split_needed {
            let (_left, right) =
                self.shard_directory
                    .split_shard(hottest_shard)
                    .map_err(|err| {
                        DbError::invalid_argument(format!("autoscale split failed: {err:?}"))
                    })?;
            if let Some(previous) = self.shard_write_ops.get(&hottest_shard).copied() {
                let left = previous / 2;
                let right_ops = previous.saturating_sub(left);
                self.shard_write_ops.insert(hottest_shard, left);
                self.shard_write_ops.insert(right, right_ops);
            }
            self.persist_topology_state_required()?;
            self.record_autoscale_status(
                "split_shard",
                vec![
                    format!("hottest shard {hottest_shard} ratio {:.3}", hottest_ratio),
                    format!(
                        "logical shards now {}",
                        self.shard_directory.logical_shard_count()
                    ),
                ],
                now_ms,
            );
        } else {
            self.record_autoscale_status(
                "hold",
                vec![
                    format!("hottest ratio {:.3} within threshold", hottest_ratio),
                    format!(
                        "shards/group {:.2} target {}",
                        shard_density, self.autoscale_target_shards_per_group
                    ),
                ],
                now_ms,
            );
        }

        self.shard_write_ops.clear();
        Ok(self.autoscale_status())
    }

    fn autoscale_tick(&mut self) -> Result<DbAutoscaleStatus, DbError> {
        self.run_autoscale_tick(true, None)
    }

    fn autoscale_tick_background_with_nodes(
        &mut self,
        healthy_nodes: Vec<u64>,
    ) -> Result<DbAutoscaleStatus, DbError> {
        self.run_autoscale_tick(false, Some(healthy_nodes))
    }

    #[cfg(test)]
    fn inject_autoscale_healthy_nodes(&mut self, nodes: Vec<u64>) {
        self.autoscale_test_healthy_nodes = Some(normalize_healthy_nodes(nodes));
    }

    #[cfg(test)]
    fn inject_cdc_checkpoint_persist_failure(&mut self) {
        self.fail_next_cdc_checkpoint_persist = true;
    }
}

struct DbRegistry {
    next_handle: AtomicI64,
    handles: Mutex<HashMap<i64, Arc<RwLock<DbEngine>>>>,
    writers: Mutex<HashMap<i64, Arc<WriteLanePool>>>,
    apply_lanes: Mutex<HashMap<i64, Arc<ApplyLanePool>>>,
    autoscalers: Mutex<HashMap<i64, Arc<AutoscaleLane>>>,
    meshes: Mutex<HashMap<i64, Arc<PrivateMeshContext>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PrivateMeshSnapshot {
    leader_node_id: String,
    cluster_nodes: Vec<String>,
    addresses: HashMap<String, String>,
}

#[derive(Debug, Clone)]
struct PrivateMeshDiscoveryConfig {
    app_name: String,
    private_rpc_port: u16,
    refresh_interval: Duration,
    lookup_timeout: Duration,
    leader_override: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PrivateMeshReadiness {
    ready: bool,
    reason: String,
    last_refresh_epoch_ms: u64,
    node_count: usize,
    min_ready_nodes: usize,
    leader_node_id: String,
}

struct PrivateMeshContext {
    local_node_id: String,
    snapshot: Arc<RwLock<PrivateMeshSnapshot>>,
    readiness: Arc<RwLock<PrivateMeshReadiness>>,
    discovery: Option<PrivateMeshDiscoveryConfig>,
    strict_mesh_ready: bool,
    min_ready_nodes: usize,
    io_timeout: Duration,
    edge_service: Arc<RwLock<crate::db::rpc::grpc::GrpcEdgeService>>,
    server: Mutex<Option<crate::db::rpc::private_network::PrivateRpcServer>>,
    refresh_stop: Arc<AtomicBool>,
    refresh_thread: Mutex<Option<JoinHandle<()>>>,
}

impl PrivateMeshContext {
    fn snapshot(&self) -> Option<PrivateMeshSnapshot> {
        self.snapshot.read().ok().map(|snapshot| snapshot.clone())
    }

    fn is_leader(&self) -> bool {
        self.snapshot()
            .map(|snapshot| snapshot.leader_node_id == self.local_node_id)
            .unwrap_or(false)
    }

    fn readiness(&self) -> Option<PrivateMeshReadiness> {
        self.readiness
            .read()
            .ok()
            .map(|readiness| readiness.clone())
    }

    fn update_readiness_from_snapshot(
        &self,
        snapshot: &PrivateMeshSnapshot,
        reason_prefix: &str,
    ) -> Result<(), String> {
        let readiness = mesh_readiness_from_snapshot(
            snapshot,
            self.strict_mesh_ready,
            self.min_ready_nodes,
            reason_prefix,
        );
        let mut guard = self
            .readiness
            .write()
            .map_err(|_| "private mesh readiness lock poisoned".to_string())?;
        *guard = readiness;
        Ok(())
    }

    fn ensure_ready_for(&self, operation: &str) -> Result<(), DbError> {
        let Some(readiness) = self.readiness() else {
            return Err(DbError::limit(format!(
                "private mesh readiness unavailable for {operation}; RETRY_AFTER_MS=25"
            )));
        };
        if readiness.ready {
            return Ok(());
        }
        Err(DbError::limit(format!(
            "private mesh not ready for {operation}: {}; RETRY_AFTER_MS=25",
            readiness.reason
        )))
    }

    fn leader_address(&self) -> Option<String> {
        let snapshot = self.snapshot()?;
        snapshot.addresses.get(&snapshot.leader_node_id).cloned()
    }

    fn follower_nodes(&self) -> Vec<String> {
        let Some(snapshot) = self.snapshot() else {
            return Vec::new();
        };
        snapshot
            .cluster_nodes
            .iter()
            .filter(|node| node.as_str() != self.local_node_id.as_str())
            .cloned()
            .collect()
    }

    fn address_for_node(&self, node_id: &str) -> Option<String> {
        self.snapshot()
            .and_then(|snapshot| snapshot.addresses.get(node_id).cloned())
    }

    fn status(&self) -> DbPrivateMeshStatus {
        let snapshot = self.snapshot().unwrap_or_else(|| PrivateMeshSnapshot {
            leader_node_id: self.local_node_id.clone(),
            cluster_nodes: vec![self.local_node_id.clone()],
            addresses: HashMap::new(),
        });
        let readiness = self
            .readiness()
            .unwrap_or_else(|| mesh_readiness_from_snapshot(&snapshot, false, 1, "unknown"));
        DbPrivateMeshStatus {
            mesh_ready: readiness.ready,
            reason: readiness.reason,
            machine_id: self.local_node_id.clone(),
            leader_id: readiness.leader_node_id,
            node_count: readiness.node_count,
            min_ready_nodes: readiness.min_ready_nodes.max(1),
            nodes: snapshot.cluster_nodes,
            last_refresh_epoch_ms: readiness.last_refresh_epoch_ms,
        }
    }

    fn refresh_membership_snapshot(&self) -> Result<(), String> {
        let Some(discovery) = self.discovery.as_ref() else {
            return Ok(());
        };
        let previous = self
            .snapshot()
            .ok_or_else(|| "private mesh discovery failed to read previous snapshot".to_string())?;
        let next = discover_mesh_snapshot(&self.local_node_id, discovery)?;
        if next == previous {
            self.update_readiness_from_snapshot(&next, "dns_refresh")?;
            return Ok(());
        }
        {
            let mut guard = self
                .snapshot
                .write()
                .map_err(|_| "private mesh snapshot lock poisoned".to_string())?;
            *guard = next.clone();
        }
        if let Ok(mut edge_service) = self.edge_service.write() {
            edge_service.set_leader_node_id(next.leader_node_id.clone());
        }
        self.update_readiness_from_snapshot(&next, "dns_refresh")?;
        runtime_startup_trace(format!(
            "private mesh: refreshed membership leader={} nodes={} addresses={} ready={}",
            next.leader_node_id,
            next.cluster_nodes.len(),
            next.addresses.len(),
            self.readiness().map(|state| state.ready).unwrap_or(false)
        ));
        Ok(())
    }

    fn start_refresh_worker(self: &Arc<Self>) {
        let Some(discovery) = self.discovery.as_ref() else {
            return;
        };
        let interval = discovery.refresh_interval;
        let mesh = Arc::clone(self);
        let stop = self.refresh_stop.clone();
        let worker = thread::Builder::new()
            .name(format!("wrela-db-mesh-refresh-{}", self.local_node_id))
            .spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let mut slept = Duration::ZERO;
                    while slept < interval {
                        if stop.load(Ordering::Relaxed) {
                            return;
                        }
                        let remaining = interval.saturating_sub(slept);
                        let slice = remaining.min(Duration::from_millis(100));
                        thread::sleep(slice);
                        slept += slice;
                    }
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    if let Err(err) = mesh.refresh_membership_snapshot() {
                        runtime_startup_trace(format!(
                            "private mesh: discovery refresh failed: {err}"
                        ));
                    }
                }
            });
        match worker {
            Ok(worker) => {
                if let Ok(mut guard) = self.refresh_thread.lock() {
                    *guard = Some(worker);
                }
            }
            Err(err) => {
                runtime_startup_trace(format!(
                    "private mesh: failed to spawn discovery refresh worker: {err}"
                ));
            }
        }
    }

    fn shutdown(&self) {
        self.refresh_stop.store(true, Ordering::Relaxed);
        if let Ok(mut guard) = self.refresh_thread.lock()
            && let Some(worker) = guard.take()
        {
            let _ = worker.join();
        }
        if let Ok(mut guard) = self.server.lock()
            && let Some(server) = guard.as_mut()
        {
            server.shutdown();
        }
    }
}

fn private_rpc_enabled() -> bool {
    std::env::var("WRELADB_PRIVATE_RPC_ENABLED")
        .ok()
        .map(|raw| {
            matches!(
                raw.trim(),
                "1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON"
            )
        })
        .unwrap_or(false)
}

const PRIVATE_RPC_DISCOVERY_REFRESH_MS_DEFAULT: u64 = 5_000;
const PRIVATE_RPC_DISCOVERY_TIMEOUT_MS_DEFAULT: u64 = 1_000;
const PRIVATE_RPC_MIN_READY_NODES_DEFAULT: usize = 3;

fn parse_cluster_nodes_env() -> Vec<String> {
    let mut nodes = std::env::var("WRELADB_CLUSTER_NODES")
        .ok()
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|node| !node.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    nodes.sort();
    nodes.dedup();
    nodes
}

fn parse_private_rpc_address_map() -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some(raw) = std::env::var("WRELADB_PRIVATE_RPC_ADDRESS_MAP").ok() else {
        return out;
    };
    for part in raw.split(',') {
        let Some((node, addr)) = part.split_once('=') else {
            continue;
        };
        let node = node.trim();
        let addr = addr.trim();
        if node.is_empty() || addr.is_empty() {
            continue;
        }
        out.insert(node.to_string(), addr.to_string());
    }
    out
}

fn parse_duration_ms_env(name: &str, default_ms: u64, min_ms: u64) -> Duration {
    let value = std::env::var(name)
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .unwrap_or(default_ms);
    Duration::from_millis(value.max(min_ms))
}

fn parse_positive_usize(raw: Option<&str>) -> Option<usize> {
    raw.and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
}

fn private_rpc_min_ready_nodes_from(
    target_voters_raw: Option<&str>,
    min_ready_raw: Option<&str>,
) -> usize {
    let derived_default = parse_positive_usize(target_voters_raw)
        .unwrap_or(PRIVATE_RPC_MIN_READY_NODES_DEFAULT)
        .max(1);
    parse_positive_usize(min_ready_raw)
        .unwrap_or(derived_default)
        .max(1)
}

fn parse_private_rpc_min_ready_nodes() -> usize {
    private_rpc_min_ready_nodes_from(
        std::env::var("WRELADB_TARGET_VOTERS").ok().as_deref(),
        std::env::var("WRELADB_PRIVATE_RPC_MIN_READY_NODES")
            .ok()
            .as_deref(),
    )
}

fn leader_override_env() -> Option<String> {
    std::env::var("WRELADB_LEADER_NODE_ID")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Selects the initial leader for cluster bootstrap only. This heuristic picks
/// the first sorted node or an env override. Raft election (`election.rs`)
/// handles all steady-state leader selection after the cluster forms.
fn choose_bootstrap_leader_node(leader_override: Option<&str>, cluster_nodes: &[String]) -> String {
    if let Some(leader_override) = leader_override
        && cluster_nodes.iter().any(|node| node == leader_override)
    {
        return leader_override.to_string();
    }
    cluster_nodes.first().cloned().unwrap_or_default()
}

fn static_mesh_snapshot_for_non_fly(
    local_node_id: &str,
    app_name: Option<&str>,
    private_rpc_port: u16,
    leader_override: Option<&str>,
) -> Result<PrivateMeshSnapshot, DbError> {
    let mut cluster_nodes = parse_cluster_nodes_env();
    if !cluster_nodes.iter().any(|node| node == local_node_id) {
        cluster_nodes.push(local_node_id.to_string());
    }
    cluster_nodes.sort();
    cluster_nodes.dedup();

    let mut addresses = parse_private_rpc_address_map();
    for node in &cluster_nodes {
        if addresses.contains_key(node) {
            continue;
        }
        if node == local_node_id {
            addresses.insert(node.clone(), format!("127.0.0.1:{private_rpc_port}"));
            continue;
        }
        if let Some(app_name) = app_name {
            addresses.insert(
                node.clone(),
                format!("{node}.vm.{app_name}.internal:{private_rpc_port}"),
            );
        }
    }

    let leader_node_id = choose_bootstrap_leader_node(leader_override, &cluster_nodes);
    if !addresses.contains_key(&leader_node_id) {
        return Err(DbError::invalid_argument(format!(
            "private rpc enabled but no address for leader node `{leader_node_id}`"
        )));
    }

    Ok(PrivateMeshSnapshot {
        leader_node_id,
        cluster_nodes,
        addresses,
    })
}

fn mesh_snapshot_from_nodes(
    local_node_id: &str,
    mut cluster_nodes: Vec<String>,
    discovery: &PrivateMeshDiscoveryConfig,
) -> Result<PrivateMeshSnapshot, String> {
    if !cluster_nodes.iter().any(|node| node == local_node_id) {
        cluster_nodes.push(local_node_id.to_string());
    }
    cluster_nodes.sort();
    cluster_nodes.dedup();
    if cluster_nodes.is_empty() {
        return Err("dns discovery returned empty cluster node set".to_string());
    }

    let mut addresses = crate::db::rpc::private_network::fly_private_rpc_addresses(
        &cluster_nodes,
        &discovery.app_name,
        discovery.private_rpc_port,
    );
    let loopback_addr = format!("127.0.0.1:{}", discovery.private_rpc_port);
    addresses.insert(local_node_id.to_string(), loopback_addr);

    let leader_node_id =
        choose_bootstrap_leader_node(discovery.leader_override.as_deref(), &cluster_nodes);
    if !addresses.contains_key(&leader_node_id) {
        return Err(format!(
            "discovered snapshot missing leader address for {leader_node_id}"
        ));
    }
    Ok(PrivateMeshSnapshot {
        leader_node_id,
        cluster_nodes,
        addresses,
    })
}

fn discover_mesh_snapshot(
    local_node_id: &str,
    discovery: &PrivateMeshDiscoveryConfig,
) -> Result<PrivateMeshSnapshot, String> {
    let cluster_nodes = crate::db::rpc::private_network::discover_fly_machine_ids_via_dns(
        &discovery.app_name,
        discovery.lookup_timeout,
    )?;
    mesh_snapshot_from_nodes(local_node_id, cluster_nodes, discovery)
}

fn mesh_readiness_from_snapshot(
    snapshot: &PrivateMeshSnapshot,
    strict_mesh_ready: bool,
    min_ready_nodes: usize,
    reason_prefix: &str,
) -> PrivateMeshReadiness {
    let node_count = snapshot.cluster_nodes.len();
    let min_ready_nodes = min_ready_nodes.max(1);
    let (ready, reason) = if strict_mesh_ready {
        if node_count >= min_ready_nodes {
            (
                true,
                format!("{reason_prefix}: discovered {node_count}/{min_ready_nodes} nodes"),
            )
        } else {
            (
                false,
                format!("{reason_prefix}: waiting for nodes {node_count}/{min_ready_nodes}"),
            )
        }
    } else {
        (
            true,
            format!("{reason_prefix}: static mesh mode nodes={node_count}"),
        )
    };

    PrivateMeshReadiness {
        ready,
        reason,
        last_refresh_epoch_ms: now_epoch_ms(),
        node_count,
        min_ready_nodes,
        leader_node_id: snapshot.leader_node_id.clone(),
    }
}

fn sanitize_idempotency_token_component(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "unknown".to_string()
    } else {
        out
    }
}

fn private_mesh_idempotency_token(prefix: &str, handle: i64) -> String {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let local_node_id = sanitize_idempotency_token_component(&private_mesh_local_node_id());
    format!(
        "{prefix}-{local_node_id}-{}-{handle}-{}-{}",
        std::process::id(),
        now_epoch_ms(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

fn default_private_rpc_bind_addr(private_rpc_port: u16, app_name: Option<&str>) -> String {
    if app_name.is_some() {
        return format!("[::]:{private_rpc_port}");
    }
    format!("0.0.0.0:{private_rpc_port}")
}

fn map_private_rpc_error(err: crate::db::rpc::errors::RpcError) -> DbError {
    let retry = err
        .retry
        .as_ref()
        .map(|hint| format!("; RETRY_AFTER_MS={}", hint.retry_after_ms))
        .unwrap_or_default();
    if err.message.contains("REPLICATION_RPC_BACKPRESSURE") {
        return DbError::limit(format!(
            "{}: private rpc failed: {}{}",
            QUORUM_FAILURE_TOKEN_REPLICATION_RPC_BACKPRESSURE, err.message, retry
        ));
    }
    let token = match err.code {
        crate::db::rpc::errors::RpcStatusCode::Unavailable => {
            QUORUM_FAILURE_TOKEN_PRIVATE_RPC_UNAVAILABLE
        }
        crate::db::rpc::errors::RpcStatusCode::RetryAfter => {
            QUORUM_FAILURE_TOKEN_PRIVATE_RPC_RETRY_AFTER
        }
        crate::db::rpc::errors::RpcStatusCode::NotLeader => {
            QUORUM_FAILURE_TOKEN_PRIVATE_RPC_NOT_LEADER
        }
        crate::db::rpc::errors::RpcStatusCode::OccMismatch => {
            QUORUM_FAILURE_TOKEN_PRIVATE_RPC_OCC_MISMATCH
        }
        crate::db::rpc::errors::RpcStatusCode::InvalidArgument => {
            QUORUM_FAILURE_TOKEN_PRIVATE_RPC_INVALID_ARGUMENT
        }
    };
    DbError::limit(format!(
        "{token}: private rpc failed: {}{}",
        err.message, retry
    ))
}

fn record_client_write_path_sample_best_effort(handle: i64, sample: DbClientWritePathSample) {
    let _ = with_engine_mut(handle, |engine| {
        engine.record_client_write_path_sample(sample);
        Ok(())
    });
}

fn record_insert_fast_lane_attempt_best_effort(handle: i64, accepted: bool) {
    let _ = with_engine_mut(handle, |engine| {
        engine.record_insert_fast_lane_attempt(accepted);
        Ok(())
    });
}

fn private_mesh_local_node_id() -> String {
    std::env::var("WRELADB_NODE_ID")
        .or_else(|_| std::env::var("FLY_MACHINE_ID"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "local".to_string())
}

fn maybe_initialize_private_mesh(handle: i64) -> Result<Option<Arc<PrivateMeshContext>>, DbError> {
    if !private_rpc_enabled() {
        runtime_startup_trace("private mesh: disabled");
        return Ok(None);
    }

    let local_node_id = private_mesh_local_node_id();
    let private_rpc_port = std::env::var("WRELADB_PRIVATE_RPC_PORT")
        .ok()
        .and_then(|raw| raw.parse::<u16>().ok())
        .unwrap_or(19_091);
    let io_timeout_ms = std::env::var("WRELADB_PRIVATE_RPC_TIMEOUT_MS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .unwrap_or(2_000)
        .max(100);
    let io_timeout = Duration::from_millis(io_timeout_ms);
    let app_name = std::env::var("FLY_APP_NAME")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let bind_addr = std::env::var("WRELADB_PRIVATE_RPC_BIND")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default_private_rpc_bind_addr(private_rpc_port, app_name.as_deref()));
    let fly_discovery_enabled = app_name.is_some();
    let leader_override = leader_override_env();
    let min_ready_nodes = parse_private_rpc_min_ready_nodes();
    let mtls_mode_raw = std::env::var("WRELADB_PRIVATE_RPC_MTLS_MODE").ok();
    let trusted_network_raw = std::env::var("WRELADB_PRIVATE_RPC_TRUSTED_NETWORK").ok();
    let security_policy = resolve_private_rpc_security_policy(
        mtls_mode_raw.as_deref(),
        trusted_network_raw.as_deref(),
        fly_discovery_enabled,
        false,
    )
    .map_err(|err| {
        DbError::invalid_argument(format!("private rpc security policy invalid: {err}"))
    })?;

    runtime_startup_trace(format!(
        "private mesh: enabled handle={handle} local_node_id={local_node_id} bind_addr={bind_addr} app_name={} port={private_rpc_port} mtls_mode={} mtls_effective={} trusted_network={}",
        app_name.as_deref().unwrap_or("<none>"),
        security_policy.configured_mode.as_str(),
        security_policy.effective_mtls_enabled,
        security_policy
            .trusted_network
            .as_deref()
            .unwrap_or("<none>")
    ));

    let fallback_snapshot = if fly_discovery_enabled {
        PrivateMeshSnapshot {
            leader_node_id: local_node_id.clone(),
            cluster_nodes: vec![local_node_id.clone()],
            addresses: HashMap::from([(
                local_node_id.clone(),
                format!("127.0.0.1:{private_rpc_port}"),
            )]),
        }
    } else {
        static_mesh_snapshot_for_non_fly(
            &local_node_id,
            app_name.as_deref(),
            private_rpc_port,
            leader_override.as_deref(),
        )?
    };

    let discovery = app_name
        .as_ref()
        .map(|app_name| PrivateMeshDiscoveryConfig {
            app_name: app_name.clone(),
            private_rpc_port,
            refresh_interval: parse_duration_ms_env(
                "WRELADB_PRIVATE_RPC_DISCOVERY_REFRESH_MS",
                PRIVATE_RPC_DISCOVERY_REFRESH_MS_DEFAULT,
                250,
            ),
            lookup_timeout: parse_duration_ms_env(
                "WRELADB_PRIVATE_RPC_DISCOVERY_TIMEOUT_MS",
                PRIVATE_RPC_DISCOVERY_TIMEOUT_MS_DEFAULT,
                100,
            ),
            leader_override: leader_override.clone(),
        });
    let snapshot = Arc::new(RwLock::new(fallback_snapshot.clone()));
    let strict_mesh_ready = fly_discovery_enabled;
    let readiness = Arc::new(RwLock::new(mesh_readiness_from_snapshot(
        &fallback_snapshot,
        strict_mesh_ready,
        min_ready_nodes,
        "startup",
    )));
    runtime_startup_trace(format!(
        "private mesh: leader={} cluster_nodes={} addresses={} ready={} min_ready_nodes={}",
        fallback_snapshot.leader_node_id,
        fallback_snapshot.cluster_nodes.len(),
        fallback_snapshot.addresses.len(),
        readiness
            .read()
            .ok()
            .map(|state| state.ready)
            .unwrap_or(false),
        min_ready_nodes
    ));

    let resolver_snapshot = snapshot.clone();
    let resolver: crate::db::rpc::private_network::NodeAddressResolver = Arc::new(move |node_id| {
        resolver_snapshot
            .read()
            .ok()
            .and_then(|state| state.addresses.get(node_id).cloned())
    });

    let mut edge_service = crate::db::rpc::grpc::GrpcEdgeService::new(
        local_node_id.clone(),
        fallback_snapshot.leader_node_id.clone(),
    );
    edge_service.bind_handle(handle);
    edge_service.set_remote_write_transport(Some(
        crate::db::rpc::private_network::build_private_write_transport(resolver, io_timeout),
    ));
    let edge_service = Arc::new(RwLock::new(edge_service));
    runtime_startup_trace("private mesh: starting private rpc server");
    let server = crate::db::rpc::private_network::start_private_rpc_server(
        &bind_addr,
        edge_service.clone(),
        io_timeout,
    )
    .map_err(|err| DbError::io(format!("private rpc init failed: {err}")))?;
    runtime_startup_trace(format!(
        "private mesh: private rpc server listening at {}",
        server.listen_addr()
    ));

    let mesh = Arc::new(PrivateMeshContext {
        local_node_id,
        snapshot,
        readiness,
        discovery,
        strict_mesh_ready,
        min_ready_nodes,
        io_timeout,
        edge_service,
        server: Mutex::new(Some(server)),
        refresh_stop: Arc::new(AtomicBool::new(false)),
        refresh_thread: Mutex::new(None),
    });
    if fly_discovery_enabled {
        if let Err(err) = mesh.refresh_membership_snapshot() {
            runtime_startup_trace(format!(
                "private mesh: startup dns discovery failed, using fallback snapshot: {err}"
            ));
        }
        mesh.start_refresh_worker();
    }

    Ok(Some(mesh))
}

fn next_positive_i64_handle(counter: &AtomicI64) -> Option<i64> {
    loop {
        let current = counter.load(Ordering::Relaxed);
        if current <= 0 {
            return None;
        }
        let next = if current == i64::MAX { 0 } else { current + 1 };
        if counter
            .compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            return Some(current);
        }
    }
}

impl DbRegistry {
    fn new() -> Self {
        Self {
            next_handle: AtomicI64::new(1),
            handles: Mutex::new(HashMap::new()),
            writers: Mutex::new(HashMap::new()),
            apply_lanes: Mutex::new(HashMap::new()),
            autoscalers: Mutex::new(HashMap::new()),
            meshes: Mutex::new(HashMap::new()),
        }
    }
}

fn registry() -> &'static DbRegistry {
    static REGISTRY: OnceLock<DbRegistry> = OnceLock::new();
    REGISTRY.get_or_init(DbRegistry::new)
}

fn wal_path_from(data_dir: &Path) -> PathBuf {
    data_dir.join("wal.log")
}

fn wal_lane_path_from(data_dir: &Path, lane_id: usize) -> PathBuf {
    data_dir.join(format!("wal-lane-{lane_id}.log"))
}

fn should_restore_latest_checkpoint_on_open(
    checkpoint_config: &crate::db::checkpoint::CheckpointConfig,
) -> bool {
    if matches!(
        checkpoint_config.backend,
        crate::db::checkpoint::CheckpointBackend::S3
    ) {
        return true;
    }
    if checkpoint_config.checkpoint_dir.join("LATEST").exists() {
        return true;
    }
    let checkpoints_dir = checkpoint_config.checkpoint_dir.join("checkpoints");
    match std::fs::read_dir(checkpoints_dir) {
        Ok(entries) => entries.filter_map(Result::ok).next().is_some(),
        Err(_) => false,
    }
}

fn resolve_checkpoint_config(
    data_dir: &Path,
    config: &crate::db::checkpoint::CheckpointConfig,
) -> crate::db::checkpoint::CheckpointConfig {
    let mut resolved = config.clone();
    if resolved.checkpoint_dir.is_relative() {
        resolved.checkpoint_dir = data_dir.join(&resolved.checkpoint_dir);
    }
    resolved
}

fn cdc_checkpoint_path_from(wal_path: &Path) -> PathBuf {
    wal_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("cdc_checkpoints.json")
}

fn load_cdc_checkpoints(wal_path: &Path) -> Result<crate::db::cdc::CdcCheckpointStore, DbError> {
    let path = cdc_checkpoint_path_from(wal_path);
    if !path.exists() {
        return Ok(crate::db::cdc::CdcCheckpointStore::default());
    }
    let payload = std::fs::read(&path).map_err(|err| DbError::io(err.to_string()))?;
    let checkpoints: HashMap<String, u64> =
        serde_json::from_slice(&payload).map_err(|err| DbError::io(err.to_string()))?;
    Ok(crate::db::cdc::CdcCheckpointStore::from_checkpoints(
        checkpoints,
    ))
}

fn persist_cdc_checkpoints(
    wal_path: &Path,
    store: &crate::db::cdc::CdcCheckpointStore,
    #[cfg(test)] fail_dir_fsync: bool,
) -> Result<(), DbError> {
    let path = cdc_checkpoint_path_from(wal_path);
    let payload = serde_json::to_vec_pretty(store.checkpoints())
        .map_err(|err| DbError::io(err.to_string()))?;
    let tmp = path.with_extension("json.tmp");
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&tmp)
        .map_err(|err| DbError::io(err.to_string()))?;
    file.write_all(&payload)
        .map_err(|err| DbError::io(err.to_string()))?;
    file.sync_data()
        .map_err(|err| DbError::io(err.to_string()))?;
    std::fs::rename(&tmp, &path).map_err(|err| DbError::io(err.to_string()))?;
    #[cfg(test)]
    if fail_dir_fsync {
        return Err(DbError::io("injected cdc checkpoint dir fsync failure"));
    }
    fsync_parent_dir(&path)?;
    Ok(())
}

fn fsync_parent_dir(path: &Path) -> Result<(), DbError> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let dir = File::open(parent).map_err(|err| DbError::io(err.to_string()))?;
    dir.sync_all().map_err(|err| DbError::io(err.to_string()))
}

fn saturating_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn compile_db_intent_for_open(config: &DbConfig) -> Result<(), DbError> {
    let available_nodes = saturating_u32(
        config
            .topology
            .region_az_node_map
            .values()
            .flat_map(|az_map| az_map.values())
            .map(std::vec::Vec::len)
            .sum::<usize>(),
    );
    let topology_hints = crate::db::autopilot::compiler::DbIntentTopologyHints {
        available_nodes,
        logical_shards: config.topology.initial_logical_shards,
    };
    crate::db::autopilot::compiler::compile_db_intent(&config.intent, topology_hints).map_err(
        |err| {
            DbError::invalid_argument(format!(
                "STRICT_CONFIG_INVALID: intent invalid for topology hints (available_nodes={}, logical_shards={}): {}",
                topology_hints.available_nodes,
                topology_hints.logical_shards,
                format_db_intent_compile_error(&err)
            ))
        },
    )?;
    Ok(())
}

fn format_db_intent_compile_error(
    err: &crate::db::autopilot::compiler::DbIntentCompilerError,
) -> String {
    match err {
        crate::db::autopilot::compiler::DbIntentCompilerError::Contradiction(contradiction) => {
            let remediations = contradiction
                .remediations
                .iter()
                .map(|remediation| remediation.detail.as_str())
                .collect::<Vec<_>>();
            if remediations.is_empty() {
                format!(
                    "intent contradiction {:?}: {}",
                    contradiction.code, contradiction.reason
                )
            } else {
                format!(
                    "intent contradiction {:?}: {}; remediation: {}",
                    contradiction.code,
                    contradiction.reason,
                    remediations.join(" | ")
                )
            }
        }
        other => format!("intent invalid: {other:?}"),
    }
}

fn append_bytes_part(out: &mut Vec<u8>, part: &[u8]) {
    out.extend_from_slice(&(part.len() as u32).to_be_bytes());
    out.extend_from_slice(part);
}

fn command_payload(command: &RaftCommand) -> Vec<u8> {
    let mut out = Vec::new();
    match command {
        RaftCommand::Put {
            namespace,
            key,
            value,
            expected_version,
        } => {
            out.push(b'P');
            append_bytes_part(&mut out, namespace);
            append_bytes_part(&mut out, key);
            append_bytes_part(&mut out, value);
            out.extend_from_slice(&expected_version.unwrap_or(0).to_be_bytes());
        }
        RaftCommand::Delete {
            namespace,
            key,
            expected_version,
        } => {
            out.push(b'D');
            append_bytes_part(&mut out, namespace);
            append_bytes_part(&mut out, key);
            out.extend_from_slice(&expected_version.unwrap_or(0).to_be_bytes());
        }
    }
    out
}

fn replicate_to_follower(
    leader: &NodeState,
    follower: &mut NodeState,
    leader_commit: u64,
) -> Result<AppendEntriesResponse, DbError> {
    let mut next_index = follower
        .last_log_index()
        .saturating_add(1)
        .min(leader.last_log_index().saturating_add(1));
    let max_attempts = leader.last_log_index().saturating_add(2).max(1);
    let mut attempts = 0u64;
    while attempts < max_attempts {
        attempts = attempts.saturating_add(1);
        let prev_log_index = next_index.saturating_sub(1);
        let prev_log_term = leader.log_term_at(prev_log_index).unwrap_or(0);
        let entries = {
            let log = &leader.log;
            let start = log.partition_point(|e| e.index < next_index);
            log.get(start..).unwrap_or_default().to_vec()
        };
        let req = AppendEntries {
            term: leader.current_term,
            leader_id: leader.node_id,
            prev_log_index,
            prev_log_term,
            leader_commit,
            entries,
        };
        let result = handle_append_entries(follower, &req, 0, 10);
        if result.response.success {
            return Ok(result.response);
        }
        if result.response.term > leader.current_term {
            return Ok(result.response);
        }
        let Some(conflict_index) = result.response.conflict_index else {
            return Ok(result.response);
        };
        let bounded_conflict = conflict_index.min(leader.last_log_index().saturating_add(1));
        if bounded_conflict >= next_index {
            return Err(DbError::limit(format!(
                "replication convergence stalled at index {next_index}; RETRY_AFTER_MS=25"
            )));
        }
        next_index = bounded_conflict;
    }

    Err(DbError::limit(format!(
        "replication convergence exceeded attempt bound {}; RETRY_AFTER_MS=25",
        max_attempts
    )))
}

fn db_for_handle(handle: i64) -> Result<Arc<RwLock<DbEngine>>, DbError> {
    lock_registry_handles()?
        .get(&handle)
        .cloned()
        .ok_or_else(|| DbError::invalid_argument("unknown DB handle"))
}

fn lock_registry_handles()
-> Result<MutexGuard<'static, HashMap<i64, Arc<RwLock<DbEngine>>>>, DbError> {
    registry()
        .handles
        .lock()
        .map_err(|_| DbError::io("DB registry lock poisoned"))
}

fn lock_registry_writers() -> Result<MutexGuard<'static, HashMap<i64, Arc<WriteLanePool>>>, DbError>
{
    registry()
        .writers
        .lock()
        .map_err(|_| DbError::io("DB writer registry lock poisoned"))
}

fn lock_registry_apply_lanes()
-> Result<MutexGuard<'static, HashMap<i64, Arc<ApplyLanePool>>>, DbError> {
    registry()
        .apply_lanes
        .lock()
        .map_err(|_| DbError::io("DB apply-lane registry lock poisoned"))
}

fn lock_registry_autoscalers()
-> Result<MutexGuard<'static, HashMap<i64, Arc<AutoscaleLane>>>, DbError> {
    registry()
        .autoscalers
        .lock()
        .map_err(|_| DbError::io("DB autoscale registry lock poisoned"))
}

fn lock_registry_meshes()
-> Result<MutexGuard<'static, HashMap<i64, Arc<PrivateMeshContext>>>, DbError> {
    registry()
        .meshes
        .lock()
        .map_err(|_| DbError::io("DB mesh registry lock poisoned"))
}

fn writer_pool_for_handle(handle: i64) -> Result<Arc<WriteLanePool>, DbError> {
    lock_registry_writers()?
        .get(&handle)
        .cloned()
        .ok_or_else(|| DbError::invalid_argument("unknown DB handle"))
}

fn writer_for_shard(handle: i64, logical_shard: u32) -> Result<Arc<WriteLane>, DbError> {
    Ok(writer_pool_for_handle(handle)?.lane_for_shard(logical_shard))
}

fn apply_lane_pool_for_handle(handle: i64) -> Result<Arc<ApplyLanePool>, DbError> {
    lock_registry_apply_lanes()?
        .get(&handle)
        .cloned()
        .ok_or_else(|| DbError::invalid_argument("unknown DB handle"))
}

fn mesh_for_handle(handle: i64) -> Result<Option<Arc<PrivateMeshContext>>, DbError> {
    Ok(lock_registry_meshes()?.get(&handle).cloned())
}

fn lock_engine_read(db: &Arc<RwLock<DbEngine>>) -> Result<RwLockReadGuard<'_, DbEngine>, DbError> {
    db.read()
        .map_err(|_| DbError::io("DB engine read lock poisoned"))
}

fn lock_engine_write(
    db: &Arc<RwLock<DbEngine>>,
) -> Result<RwLockWriteGuard<'_, DbEngine>, DbError> {
    db.write()
        .map_err(|_| DbError::io("DB engine write lock poisoned"))
}

fn with_engine<T, F>(handle: i64, f: F) -> Result<T, DbError>
where
    F: FnOnce(&DbEngine) -> Result<T, DbError>,
{
    let db = db_for_handle(handle)?;
    let engine = lock_engine_read(&db)?;
    f(&engine)
}

fn with_engine_mut<T, F>(handle: i64, f: F) -> Result<T, DbError>
where
    F: FnOnce(&mut DbEngine) -> Result<T, DbError>,
{
    let db = db_for_handle(handle)?;
    let mut engine = lock_engine_write(&db)?;
    f(&mut engine)
}

struct SnapshotRestoreRuntimeState {
    db: Arc<RwLock<DbEngine>>,
    writer_lane_count: usize,
    apply_lane_count: usize,
    autoscale_tick_ms: u64,
    local_region: String,
    had_mesh: bool,
}

fn quiesce_runtime_for_snapshot_restore(
    handle: i64,
) -> Result<SnapshotRestoreRuntimeState, DbError> {
    let db = db_for_handle(handle)?;
    let (autoscale_tick_ms, local_region) = {
        let engine = lock_engine_read(&db)?;
        (engine.autoscale_tick_ms, engine.local_region.clone())
    };

    let writer_pool = {
        let mut writers = lock_registry_writers()?;
        writers
            .remove(&handle)
            .ok_or_else(|| DbError::invalid_argument("unknown DB handle"))?
    };
    let writer_lane_count = writer_pool.lane_count();
    writer_pool.shutdown();

    let apply_pool = {
        let mut apply_lanes = lock_registry_apply_lanes()?;
        apply_lanes
            .remove(&handle)
            .ok_or_else(|| DbError::invalid_argument("unknown DB handle"))?
    };
    let apply_lane_count = apply_pool.lane_count();
    apply_pool.shutdown();

    let autoscaler = {
        let mut autoscalers = lock_registry_autoscalers()?;
        autoscalers
            .remove(&handle)
            .ok_or_else(|| DbError::invalid_argument("unknown DB handle"))?
    };
    autoscaler.shutdown();

    let mesh = {
        let mut meshes = lock_registry_meshes()?;
        meshes.remove(&handle)
    };
    let had_mesh = mesh.is_some();
    if let Some(mesh) = mesh {
        mesh.shutdown();
    }

    Ok(SnapshotRestoreRuntimeState {
        db,
        writer_lane_count,
        apply_lane_count,
        autoscale_tick_ms,
        local_region,
        had_mesh,
    })
}

fn restart_runtime_after_snapshot_restore(
    handle: i64,
    state: &SnapshotRestoreRuntimeState,
) -> Result<(), DbError> {
    let writer_pool = WriteLanePool::start(handle, state.db.clone(), state.writer_lane_count)?;
    let apply_pool = match ApplyLanePool::start(handle, state.db.clone(), state.apply_lane_count) {
        Ok(pool) => pool,
        Err(err) => {
            writer_pool.shutdown();
            return Err(err);
        }
    };
    let autoscaler = match AutoscaleLane::start(
        handle,
        state.db.clone(),
        state.autoscale_tick_ms,
        state.local_region.clone(),
    ) {
        Ok(lane) => lane,
        Err(err) => {
            apply_pool.shutdown();
            writer_pool.shutdown();
            return Err(err);
        }
    };

    let maybe_mesh = if state.had_mesh {
        match maybe_initialize_private_mesh(handle)? {
            Some(mesh) => Some(mesh),
            None => {
                autoscaler.shutdown();
                apply_pool.shutdown();
                writer_pool.shutdown();
                return Err(DbError::io(
                    "private mesh restart expected but private rpc is now disabled",
                ));
            }
        }
    } else {
        None
    };

    let insert_result = (|| -> Result<(), DbError> {
        lock_registry_writers()?.insert(handle, writer_pool.clone());
        lock_registry_apply_lanes()?.insert(handle, apply_pool.clone());
        lock_registry_autoscalers()?.insert(handle, autoscaler.clone());
        if let Some(mesh) = maybe_mesh.as_ref() {
            lock_registry_meshes()?.insert(handle, mesh.clone());
        }
        Ok(())
    })();
    if let Err(err) = insert_result {
        if let Some(mesh) = maybe_mesh {
            mesh.shutdown();
        }
        autoscaler.shutdown();
        apply_pool.shutdown();
        writer_pool.shutdown();
        let _ = lock_registry_writers().map(|mut writers| {
            writers.remove(&handle);
        });
        let _ = lock_registry_apply_lanes().map(|mut apply_lanes| {
            apply_lanes.remove(&handle);
        });
        let _ = lock_registry_autoscalers().map(|mut autoscalers| {
            autoscalers.remove(&handle);
        });
        let _ = lock_registry_meshes().map(|mut meshes| {
            meshes.remove(&handle);
        });
        return Err(err);
    }

    Ok(())
}

pub fn open_db(data_dir: &Path) -> Result<i64, DbError> {
    #[cfg(test)]
    {
        return open_db_with_config(data_dir, &DbConfig::for_testing());
    }
    #[cfg(not(test))]
    {
        let config = DbConfig::from_env_strict()
            .map_err(|err| DbError::invalid_argument(format!("STRICT_CONFIG_INVALID: {err}")))?;
        open_db_with_config(data_dir, &config)
    }
}

pub fn open_db_with_config(data_dir: &Path, config: &DbConfig) -> Result<i64, DbError> {
    runtime_startup_trace(format!(
        "open_db_with_config: begin data_dir={}",
        data_dir.display()
    ));
    let mut effective = config.clone();
    if effective.residency_policy.is_none() {
        match ResidencyPolicy::from_env() {
            Ok(policy) => {
                effective.residency_policy = policy;
            }
            Err(err) => {
                return Err(DbError::sovereignty_policy_missing(format!(
                    "SOVEREIGNTY_POLICY_MISSING: failed to parse WRELADB_RESIDENCY_POLICY_JSON: {err}"
                )));
            }
        }
    }
    effective.topology.local_region = normalize_region_id(&effective.topology.local_region)
        .ok_or_else(|| {
            DbError::invalid_argument(
                "SOVEREIGNTY_REGION_UNRESOLVED: topology.local_region must be non-empty",
            )
        })?;
    effective.topology.region_az_node_map =
        canonicalize_topology_region_az_node_map(&effective.topology.region_az_node_map);
    let canonical_regions = canonical_region_set_from_map(&effective.topology.region_az_node_map);
    if !canonical_regions.contains(&effective.topology.local_region) {
        return Err(DbError::invalid_argument(
            "SOVEREIGNTY_REGION_UNRESOLVED: topology.local_region missing from canonical topology map",
        ));
    }
    effective.checkpoint.allowed_regions =
        normalize_region_list(&effective.checkpoint.allowed_regions);
    effective.sovereignty.allowed_regions =
        normalize_region_list(&effective.sovereignty.allowed_regions);
    if effective.topology.initial_logical_shards == 0 {
        return Err(DbError::invalid_argument(
            "STRICT_CONFIG_INVALID: topology.initial_logical_shards must be > 0",
        ));
    }
    if effective.topology.initial_active_groups == 0 {
        return Err(DbError::invalid_argument(
            "STRICT_CONFIG_INVALID: topology.initial_active_groups must be > 0",
        ));
    }
    if let Some(message) = strict_replication_validation_message(
        effective.replication.factor,
        effective.replication.write_quorum,
    ) {
        return Err(DbError::invalid_argument(format!(
            "STRICT_CONFIG_INVALID: {message}"
        )));
    }
    if effective.engine.writer_lane_count == 0 {
        return Err(DbError::invalid_argument(
            "STRICT_CONFIG_INVALID: engine.writer_lane_count must be > 0",
        ));
    }
    if effective.topology.autoscale_tick_ms < 100 {
        return Err(DbError::invalid_argument(
            "STRICT_CONFIG_INVALID: topology.autoscale_tick_ms must be >= 100",
        ));
    }
    if !effective.topology.autoscale_max_skew_ratio.is_finite()
        || effective.topology.autoscale_max_skew_ratio <= 1.0
    {
        return Err(DbError::invalid_argument(
            "STRICT_CONFIG_INVALID: topology.autoscale_max_skew_ratio must be finite and > 1.0",
        ));
    }
    if effective.topology.autoscale_target_shards_per_group == 0 {
        return Err(DbError::invalid_argument(
            "STRICT_CONFIG_INVALID: topology.autoscale_target_shards_per_group must be > 0",
        ));
    }
    if effective.topology.autoscale_max_active_groups == 0 {
        return Err(DbError::invalid_argument(
            "STRICT_CONFIG_INVALID: topology.autoscale_max_active_groups must be > 0",
        ));
    }
    if effective.topology.autoscale_max_logical_shards == 0 {
        return Err(DbError::invalid_argument(
            "STRICT_CONFIG_INVALID: topology.autoscale_max_logical_shards must be > 0",
        ));
    }
    compile_db_intent_for_open(&effective)?;

    std::fs::create_dir_all(data_dir).map_err(|err| DbError::io(err.to_string()))?;
    let wal_path = wal_path_from(data_dir);
    let mut engine = DbEngine::open_with_config(&wal_path, &effective)?;
    engine.run_autopilot_controller_tick("boot");
    runtime_startup_trace("open_db_with_config: engine opened");
    let handle = next_positive_i64_handle(&registry().next_handle)
        .ok_or_else(|| DbError::limit("DB registry handle space exhausted"))?;
    runtime_startup_trace(format!("open_db_with_config: allocated handle={handle}"));
    let engine = Arc::new(RwLock::new(engine));
    let writer_lane_count = effective.engine.writer_lane_count;
    let writer = WriteLanePool::start(handle, engine.clone(), writer_lane_count)?;
    let apply_lane_count = writer_lane_count.max(1);
    let apply_lanes = ApplyLanePool::start(handle, engine.clone(), apply_lane_count)?;
    runtime_startup_trace("open_db_with_config: write lane started");
    let autoscaler = AutoscaleLane::start(
        handle,
        engine.clone(),
        effective.topology.autoscale_tick_ms,
        effective.topology.local_region.clone(),
    )?;
    runtime_startup_trace("open_db_with_config: autoscale lane started");
    lock_registry_handles()?.insert(handle, engine);
    lock_registry_writers()?.insert(handle, writer);
    lock_registry_apply_lanes()?.insert(handle, apply_lanes);
    lock_registry_autoscalers()?.insert(handle, autoscaler);
    match maybe_initialize_private_mesh(handle) {
        Ok(Some(mesh)) => {
            lock_registry_meshes()?.insert(handle, mesh);
            runtime_startup_trace("open_db_with_config: private mesh initialized");
        }
        Ok(None) => {
            runtime_startup_trace("open_db_with_config: private mesh not enabled");
        }
        Err(err) => {
            let _ = close_db(handle);
            return Err(err);
        }
    }
    runtime_startup_trace(format!("open_db_with_config: success handle={handle}"));
    Ok(handle)
}

pub fn close_db(handle: i64) -> bool {
    if let Ok(mut writers) = lock_registry_writers()
        && let Some(writer) = writers.remove(&handle)
    {
        writer.shutdown();
    }
    if let Ok(mut apply_lanes) = lock_registry_apply_lanes()
        && let Some(apply_lane_pool) = apply_lanes.remove(&handle)
    {
        apply_lane_pool.shutdown();
    }
    if let Ok(mut autoscalers) = lock_registry_autoscalers()
        && let Some(autoscaler) = autoscalers.remove(&handle)
    {
        autoscaler.shutdown();
    }
    if let Ok(mut meshes) = lock_registry_meshes()
        && let Some(mesh) = meshes.remove(&handle)
    {
        mesh.shutdown();
    }
    let flush_ok = match db_for_handle(handle) {
        Ok(db) => match lock_engine_write(&db) {
            Ok(mut engine) => engine.flush_durable_state().is_ok(),
            Err(_) => false,
        },
        Err(_) => true,
    };
    let removed = match lock_registry_handles() {
        Ok(mut handles) => handles.remove(&handle).is_some(),
        Err(_) => false,
    };
    flush_ok && removed
}

pub fn submit_put(
    handle: i64,
    namespace: Vec<u8>,
    key: Vec<u8>,
    value: Vec<u8>,
    expected_version: Option<u64>,
) -> Result<u64, DbError> {
    submit_put_internal(handle, namespace, key, value, expected_version, None)
}

pub fn submit_put_with_ownership_fence(
    handle: i64,
    namespace: Vec<u8>,
    key: Vec<u8>,
    value: Vec<u8>,
    expected_version: Option<u64>,
    ownership_fence: OwnershipFence,
) -> Result<u64, DbError> {
    submit_put_internal(
        handle,
        namespace,
        key,
        value,
        expected_version,
        Some(ownership_fence),
    )
}

fn submit_put_internal(
    handle: i64,
    namespace: Vec<u8>,
    key: Vec<u8>,
    value: Vec<u8>,
    expected_version: Option<u64>,
    ownership_fence: Option<OwnershipFence>,
) -> Result<u64, DbError> {
    let total_started = Instant::now();
    let mut client_sample = DbClientWritePathSample::default();
    let put_op = BatchOp::Put {
        namespace: namespace.clone().into(),
        key: key.clone().into(),
        value: value.clone().into(),
        expected_version,
    };
    let preflight_started = Instant::now();
    DbEngine::validate_batch(std::slice::from_ref(&put_op))?;
    let mesh = mesh_for_handle(handle)?;
    if let Some(mesh) = mesh.as_ref() {
        mesh.ensure_ready_for("write")?;
    }
    let mut route_and_fence = with_engine(handle, |engine| {
        engine.authorize_write_namespace(&namespace)?;
        let route = engine.route_key_to_shard(&namespace, &key)?;
        if let Some(fence) = ownership_fence.as_ref() {
            engine.enforce_ownership_fence_for_route(&route, fence)?;
            return Ok(Some((route.logical_shard_id, fence.clone())));
        }
        Ok(Some((
            route.logical_shard_id,
            engine.current_ownership_fence_for_route(&route)?,
        )))
    })?;
    client_sample.preflight_ns = duration_to_nanos(preflight_started.elapsed());
    let (_, preflight_fence) = route_and_fence
        .clone()
        .ok_or_else(|| DbError::io("write routing unavailable for local leader"))?;
    if let Some(mesh) = mesh.as_ref()
        && !mesh.is_leader()
    {
        let leader_addr = mesh
            .leader_address()
            .ok_or_else(|| DbError::invalid_argument("private mesh leader address unavailable"))?;
        client_sample.forwarded = true;
        let remote_started = Instant::now();
        let result = crate::db::rpc::private_network::write_batch_over_private_rpc(
            &leader_addr,
            crate::db::rpc::grpc::WriteBatchRequest {
                handle: 0,
                ops: vec![put_op],
                idempotency_token: Some(private_mesh_idempotency_token("mesh-put", handle)),
                expected_home_epoch: preflight_fence.expected_home_epoch,
                expected_shard_map_epoch: preflight_fence.expected_shard_map_epoch,
                ownership_token: preflight_fence.ownership_token.clone(),
            },
            mesh.io_timeout,
        )
        .map(|response| response.commit_version)
        .map_err(map_private_rpc_error);
        client_sample.remote_forward_ns = duration_to_nanos(remote_started.elapsed());
        client_sample.total_ns = duration_to_nanos(total_started.elapsed());
        record_client_write_path_sample_best_effort(handle, client_sample);
        return result;
    }
    if route_and_fence.is_none() {
        route_and_fence = Some(with_engine(handle, |engine| {
            let route = engine.route_key_to_shard(&namespace, &key)?;
            if let Some(fence) = ownership_fence.as_ref() {
                engine.enforce_ownership_fence_for_route(&route, fence)?;
                return Ok((route.logical_shard_id, fence.clone()));
            }
            Ok((
                route.logical_shard_id,
                engine.current_ownership_fence_for_route(&route)?,
            ))
        })?);
    }
    let (logical_shard, resolved_fence) =
        route_and_fence.ok_or_else(|| DbError::io("write routing unavailable for local leader"))?;
    let bytes_hint = namespace
        .len()
        .saturating_add(key.len())
        .saturating_add(value.len());
    let writer = writer_for_shard(handle, logical_shard)?;
    let (tx, rx) = mpsc::channel();
    let message = WriteEnvelopeMessage {
        envelope: WriteEnvelope::Put {
            namespace: Bytes::from(namespace),
            key: Bytes::from(key),
            value: Bytes::from(value),
            expected_version,
            replication_mode: ReplicationCommitMode::Quorum,
            ownership_fence: resolved_fence,
        },
        #[cfg(test)]
        kind: WriteEnvelopeKind::Put,
        logical_shard,
        ops_hint: 1,
        bytes_hint,
        oversize_atomic: false,
        enqueued_at: Instant::now(),
        response_tx: tx,
    };
    let enqueue_started = Instant::now();
    let enqueue_result = writer.enqueue(message);
    client_sample.enqueue_wait_ns = duration_to_nanos(enqueue_started.elapsed());
    if let Err(err) = enqueue_result {
        client_sample.total_ns = duration_to_nanos(total_started.elapsed());
        record_client_write_path_sample_best_effort(handle, client_sample);
        return Err(err);
    }
    let response_wait_started = Instant::now();
    let outcome = rx
        .recv()
        .map_err(|_| DbError::io("writer lane dropped completion"))?;
    client_sample.response_wait_ns = duration_to_nanos(response_wait_started.elapsed());
    client_sample.total_ns = duration_to_nanos(total_started.elapsed());
    let result = match outcome? {
        WriteResult::Version(version) => Ok(version),
        WriteResult::TxnCommitted => Err(DbError::io("unexpected txn completion for put")),
    };
    record_client_write_path_sample_best_effort(handle, client_sample);
    result
}

pub fn submit_put_insert_fast(
    handle: i64,
    namespace: Vec<u8>,
    key: Vec<u8>,
    value: Vec<u8>,
) -> Result<u64, DbError> {
    record_insert_fast_lane_attempt_best_effort(handle, true);
    submit_put(handle, namespace, key, value, None)
}

pub fn submit_batch(handle: i64, batch: &[BatchOp]) -> Result<u64, DbError> {
    submit_batch_internal(handle, batch, None)
}

pub fn submit_batch_with_ownership_fence(
    handle: i64,
    batch: &[BatchOp],
    ownership_fence: OwnershipFence,
) -> Result<u64, DbError> {
    submit_batch_internal(handle, batch, Some(ownership_fence))
}

fn submit_batch_internal(
    handle: i64,
    batch: &[BatchOp],
    ownership_fence: Option<OwnershipFence>,
) -> Result<u64, DbError> {
    let total_started = Instant::now();
    let mut client_sample = DbClientWritePathSample::default();
    let ops = batch.to_vec();
    let preflight_started = Instant::now();
    DbEngine::validate_batch(&ops)?;
    let mesh = mesh_for_handle(handle)?;
    if let Some(mesh) = mesh.as_ref() {
        mesh.ensure_ready_for("write")?;
    }
    let mut route_and_fence = with_engine(handle, |engine| {
        for op in &ops {
            match op {
                BatchOp::Put { namespace, .. } | BatchOp::Delete { namespace, .. } => {
                    engine.authorize_write_namespace(namespace)?;
                }
            }
        }
        let route = engine.route_batch_to_shard(&ops)?;
        if let Some(fence) = ownership_fence.as_ref() {
            engine.enforce_ownership_fence_for_route(&route, fence)?;
            return Ok(Some((route.logical_shard_id, fence.clone())));
        }
        Ok(Some((
            route.logical_shard_id,
            engine.current_ownership_fence_for_route(&route)?,
        )))
    })?;
    client_sample.preflight_ns = duration_to_nanos(preflight_started.elapsed());
    let (_, preflight_fence) = route_and_fence
        .clone()
        .ok_or_else(|| DbError::io("batch routing unavailable for local leader"))?;
    if let Some(mesh) = mesh.as_ref()
        && !mesh.is_leader()
    {
        let leader_addr = mesh
            .leader_address()
            .ok_or_else(|| DbError::invalid_argument("private mesh leader address unavailable"))?;
        client_sample.forwarded = true;
        let remote_started = Instant::now();
        let result = crate::db::rpc::private_network::write_batch_over_private_rpc(
            &leader_addr,
            crate::db::rpc::grpc::WriteBatchRequest {
                handle: 0,
                ops,
                idempotency_token: Some(private_mesh_idempotency_token("mesh-batch", handle)),
                expected_home_epoch: preflight_fence.expected_home_epoch,
                expected_shard_map_epoch: preflight_fence.expected_shard_map_epoch,
                ownership_token: preflight_fence.ownership_token.clone(),
            },
            mesh.io_timeout,
        )
        .map(|response| response.commit_version)
        .map_err(map_private_rpc_error);
        client_sample.remote_forward_ns = duration_to_nanos(remote_started.elapsed());
        client_sample.total_ns = duration_to_nanos(total_started.elapsed());
        record_client_write_path_sample_best_effort(handle, client_sample);
        return result;
    }
    if route_and_fence.is_none() {
        route_and_fence = Some(with_engine(handle, |engine| {
            let route = engine.route_batch_to_shard(&ops)?;
            if let Some(fence) = ownership_fence.as_ref() {
                engine.enforce_ownership_fence_for_route(&route, fence)?;
                return Ok((route.logical_shard_id, fence.clone()));
            }
            Ok((
                route.logical_shard_id,
                engine.current_ownership_fence_for_route(&route)?,
            ))
        })?);
    }
    let (logical_shard, resolved_fence) =
        route_and_fence.ok_or_else(|| DbError::io("batch routing unavailable for local leader"))?;
    let (ops_hint, bytes_hint) = envelope_batch_weight(&ops);
    let writer = writer_for_shard(handle, logical_shard)?;
    let (tx, rx) = mpsc::channel();
    let message = WriteEnvelopeMessage {
        envelope: WriteEnvelope::ClientBatch {
            ops,
            replication_mode: ReplicationCommitMode::Quorum,
            ownership_fence: resolved_fence,
        },
        #[cfg(test)]
        kind: WriteEnvelopeKind::ClientBatch,
        logical_shard,
        ops_hint,
        bytes_hint,
        oversize_atomic: oversize_atomic_for(true, ops_hint, bytes_hint),
        enqueued_at: Instant::now(),
        response_tx: tx,
    };
    let enqueue_started = Instant::now();
    let enqueue_result = writer.enqueue(message);
    client_sample.enqueue_wait_ns = duration_to_nanos(enqueue_started.elapsed());
    if let Err(err) = enqueue_result {
        client_sample.total_ns = duration_to_nanos(total_started.elapsed());
        record_client_write_path_sample_best_effort(handle, client_sample);
        return Err(err);
    }
    let response_wait_started = Instant::now();
    let outcome = rx
        .recv()
        .map_err(|_| DbError::io("writer lane dropped completion"))?;
    client_sample.response_wait_ns = duration_to_nanos(response_wait_started.elapsed());
    client_sample.total_ns = duration_to_nanos(total_started.elapsed());
    let result = match outcome? {
        WriteResult::Version(version) => Ok(version),
        WriteResult::TxnCommitted => Err(DbError::io("unexpected txn completion for batch")),
    };
    record_client_write_path_sample_best_effort(handle, client_sample);
    result
}

pub fn submit_batch_insert_fast(handle: i64, batch: &[BatchOp]) -> Result<u64, DbError> {
    let insert_safe = batch.iter().all(|op| {
        matches!(
            op,
            BatchOp::Put {
                expected_version: None,
                ..
            }
        )
    });
    if !insert_safe {
        record_insert_fast_lane_attempt_best_effort(handle, false);
        return submit_batch(handle, batch);
    }
    record_insert_fast_lane_attempt_best_effort(handle, true);
    submit_batch(handle, batch)
}

pub fn submit_batch_replica_local(handle: i64, batch: &[BatchOp]) -> Result<u64, DbError> {
    submit_batch_replica_local_internal(handle, batch, None)
}

pub fn submit_batch_replica_local_with_ownership_fence(
    handle: i64,
    batch: &[BatchOp],
    ownership_fence: OwnershipFence,
) -> Result<u64, DbError> {
    submit_batch_replica_local_internal(handle, batch, Some(ownership_fence))
}

fn submit_batch_replica_local_internal(
    handle: i64,
    batch: &[BatchOp],
    ownership_fence: Option<OwnershipFence>,
) -> Result<u64, DbError> {
    let ops = batch.to_vec();
    DbEngine::validate_batch(&ops)?;
    let (logical_shard, resolved_fence) = with_engine(handle, |engine| {
        for op in &ops {
            match op {
                BatchOp::Put { namespace, .. } | BatchOp::Delete { namespace, .. } => {
                    engine.authorize_write_namespace(namespace)?;
                }
            }
        }
        let route = engine.route_batch_to_shard(&ops)?;
        if let Some(fence) = ownership_fence.as_ref() {
            engine.enforce_ownership_fence_for_route(&route, fence)?;
            return Ok((route.logical_shard_id, fence.clone()));
        }
        Ok((
            route.logical_shard_id,
            engine.current_ownership_fence_for_route(&route)?,
        ))
    })?;
    let (ops_hint, bytes_hint) = envelope_batch_weight(&ops);
    let writer = writer_for_shard(handle, logical_shard)?;
    let (tx, rx) = mpsc::channel();
    let message = WriteEnvelopeMessage {
        envelope: WriteEnvelope::ClientBatch {
            ops,
            replication_mode: ReplicationCommitMode::ReplicaLocal,
            ownership_fence: resolved_fence,
        },
        #[cfg(test)]
        kind: WriteEnvelopeKind::ClientBatch,
        logical_shard,
        ops_hint,
        bytes_hint,
        oversize_atomic: oversize_atomic_for(true, ops_hint, bytes_hint),
        enqueued_at: Instant::now(),
        response_tx: tx,
    };
    writer.enqueue(message)?;
    match rx
        .recv()
        .map_err(|_| DbError::io("writer lane dropped completion"))??
    {
        WriteResult::Version(version) => Ok(version),
        WriteResult::TxnCommitted => Err(DbError::io("unexpected txn completion for batch")),
    }
}

fn replica_wal_record_matches_batch_op(record: &Record, op: &BatchOp) -> bool {
    match op {
        BatchOp::Put {
            namespace,
            key,
            value,
            ..
        } => {
            record.kind == RecordKind::Put
                && record.namespace.as_ref() == namespace.as_ref()
                && record.key.as_ref() == key.as_ref()
                && record.value.as_ref() == value.as_ref()
        }
        BatchOp::Delete { namespace, key, .. } => {
            record.kind == RecordKind::Delete
                && record.namespace.as_ref() == namespace.as_ref()
                && record.key.as_ref() == key.as_ref()
                && record.value.is_empty()
        }
    }
}

fn validate_replica_wal_payload_matches_ops(
    wal_bytes: &[u8],
    ops: &[BatchOp],
) -> Result<(), DbError> {
    if ops.is_empty() {
        return Err(DbError::invalid_argument(
            "REPLICA_WAL_DIRECT_EMPTY_BATCH: expected at least one operation",
        ));
    }
    let mut op_index = 0usize;
    let mut offset = 0usize;
    while offset < wal_bytes.len() {
        match crate::db::wal::format::decode_at(wal_bytes, offset) {
            Ok(Some((record, next))) => {
                if matches!(record.kind, RecordKind::Put | RecordKind::Delete) {
                    let op = ops.get(op_index).ok_or_else(|| {
                        DbError::invalid_argument(format!(
                            "REPLICA_WAL_PAYLOAD_EXTRA_DATA_RECORD: data_records_exceed_ops at index={op_index}"
                        ))
                    })?;
                    if !replica_wal_record_matches_batch_op(&record, op) {
                        return Err(DbError::invalid_argument(format!(
                            "REPLICA_WAL_PAYLOAD_OP_MISMATCH: data_record_index={op_index}"
                        )));
                    }
                    op_index = op_index.saturating_add(1);
                }
                offset = next;
            }
            Ok(None) => {
                return Err(DbError::invalid_argument(
                    "REPLICA_WAL_PAYLOAD_TRUNCATED: payload ended before record boundary",
                ));
            }
            Err(e) => {
                return Err(DbError::invalid_argument(format!(
                    "REPLICA_WAL_PAYLOAD_DECODE_FAILED: {e}"
                )));
            }
        }
    }
    if op_index != ops.len() {
        return Err(DbError::invalid_argument(format!(
            "REPLICA_WAL_PAYLOAD_OP_COUNT_MISMATCH: expected_ops={} data_records={op_index}",
            ops.len()
        )));
    }
    Ok(())
}

/// Follower fast-path: write pre-encoded WAL bytes directly, then apply to memtable.
/// Bypasses the writer-lane queue, WAL re-encoding, AND the WAL coordinator pipeline
/// (queue, condvar, linger, two-thread write/sync). The direct path locks the file,
/// seeks to end, writes, and fsyncs in one shot — then applies to memtable. This
/// ensures followers never expose uncommitted data in the memtable.
pub fn submit_replica_wal_direct(
    handle: i64,
    wal_bytes: &[u8],
    ops: &[BatchOp],
) -> Result<u64, DbError> {
    validate_replica_wal_payload_matches_ops(wal_bytes, ops)?;
    // Phase 1: Get the WAL reference (read lock, released immediately).
    let db = db_for_handle(handle)?;
    let wal = {
        let engine = lock_engine_read(&db)?;
        engine.wal.clone()
    };

    // Phase 2: Write + fsync WAL bytes directly (no engine lock held).
    // Bypasses the coordinator's queue/condvar/linger/two-thread pipeline.
    let ops_count = ops.len();
    wal.write_and_sync_direct(wal_bytes, ops_count)
        .map_err(|e| DbError::io(format!("follower WAL direct write failed: {e}")))?;

    // Phase 3: Apply decoded WAL records to memtable (write lock).
    // Data is already durable, so memtable only reflects committed state.
    let mut engine = lock_engine_write(&db)?;
    engine.apply_wal_records_direct(wal_bytes)
}

pub fn submit_replica_wal_direct_with_ownership_fence(
    handle: i64,
    wal_bytes: &[u8],
    ops: &[BatchOp],
    ownership_fence: OwnershipFence,
) -> Result<u64, DbError> {
    validate_replica_wal_payload_matches_ops(wal_bytes, ops)?;
    let db = db_for_handle(handle)?;
    let mut engine = lock_engine_write(&db)?;
    let route = engine.route_batch_to_shard(ops)?;
    engine.enforce_ownership_fence_for_route(&route, &ownership_fence)?;
    let ops_count = ops.len();
    engine
        .wal
        .write_and_sync_direct(wal_bytes, ops_count)
        .map_err(|e| DbError::io(format!("follower WAL direct write failed: {e}")))?;
    engine.apply_wal_records_direct(wal_bytes)
}

pub fn replica_install_sorted_run_chunk(
    handle: i64,
    term: u64,
    chunk_stream_id: u64,
    chunk_index: u64,
    total_chunks: u64,
    payload: Vec<u8>,
) -> Result<SortedRunCatchUpChunkInstallStatus, DbError> {
    with_engine_mut(handle, |engine| {
        Ok(engine.install_sorted_run_chunk(
            term,
            chunk_stream_id,
            chunk_index,
            total_chunks,
            payload,
        ))
    })
}

pub fn read_point(
    handle: i64,
    namespace: Vec<u8>,
    key: Vec<u8>,
) -> Result<Option<Vec<u8>>, DbError> {
    read_point_consistent_with_sovereignty(
        handle,
        namespace,
        key,
        ReadConsistency::Strong,
        None,
        ReadSovereigntyMode::Strict,
    )
}

pub fn read_point_consistent(
    handle: i64,
    namespace: Vec<u8>,
    key: Vec<u8>,
    consistency: ReadConsistency,
    requested_ts: Option<u64>,
) -> Result<Option<Vec<u8>>, DbError> {
    read_point_consistent_with_sovereignty(
        handle,
        namespace,
        key,
        consistency,
        requested_ts,
        ReadSovereigntyMode::Strict,
    )
}

pub fn read_point_consistent_with_sovereignty(
    handle: i64,
    namespace: Vec<u8>,
    key: Vec<u8>,
    consistency: ReadConsistency,
    requested_ts: Option<u64>,
    read_mode: ReadSovereigntyMode,
) -> Result<Option<Vec<u8>>, DbError> {
    if consistency == ReadConsistency::Strong
        && let Some(mesh) = mesh_for_handle(handle)?
    {
        mesh.ensure_ready_for("strong read")?;
        if !mesh.is_leader() {
            with_engine(handle, |engine| {
                engine.authorize_read_namespace(&namespace, read_mode, consistency)
            })?;
            let leader_addr = mesh.leader_address().ok_or_else(|| {
                DbError::invalid_argument("private mesh leader address unavailable")
            })?;
            return crate::db::rpc::private_network::point_read_over_private_rpc(
                &leader_addr,
                crate::db::rpc::grpc::PointReadRequest {
                    handle: 0,
                    namespace,
                    key,
                },
                mesh.io_timeout,
            )
            .map_err(map_private_rpc_error);
        }
    }
    with_engine(handle, |engine| {
        engine.authorize_read_namespace(&namespace, read_mode, consistency)?;
        engine.read_point(&namespace, &key, consistency, requested_ts)
    })
}

/// Reads a point and returns the version of the visible value.
/// Used for idempotency lookups (commit_version). Always reads from local state.
pub fn read_point_with_version(
    handle: i64,
    namespace: Vec<u8>,
    key: Vec<u8>,
) -> Result<Option<(u64, Vec<u8>)>, DbError> {
    with_engine(handle, |engine| {
        engine.authorize_read_namespace(
            &namespace,
            ReadSovereigntyMode::Strict,
            ReadConsistency::Strong,
        )?;
        engine.read_point_with_version(&namespace, &key, ReadConsistency::Strong, None)
    })
}

pub fn read_range(
    handle: i64,
    namespace: Vec<u8>,
    start_key: Vec<u8>,
    end_key: Vec<u8>,
    limit: usize,
) -> Result<Vec<(Vec<u8>, Vec<u8>, u64)>, DbError> {
    read_range_consistent_with_sovereignty(
        handle,
        namespace,
        start_key,
        end_key,
        limit,
        ReadConsistency::Strong,
        None,
        ReadSovereigntyMode::Strict,
    )
}

pub fn read_range_consistent(
    handle: i64,
    namespace: Vec<u8>,
    start_key: Vec<u8>,
    end_key: Vec<u8>,
    limit: usize,
    consistency: ReadConsistency,
    requested_ts: Option<u64>,
) -> Result<Vec<(Vec<u8>, Vec<u8>, u64)>, DbError> {
    read_range_consistent_with_sovereignty(
        handle,
        namespace,
        start_key,
        end_key,
        limit,
        consistency,
        requested_ts,
        ReadSovereigntyMode::Strict,
    )
}

pub fn read_range_consistent_with_sovereignty(
    handle: i64,
    namespace: Vec<u8>,
    start_key: Vec<u8>,
    end_key: Vec<u8>,
    limit: usize,
    consistency: ReadConsistency,
    requested_ts: Option<u64>,
    read_mode: ReadSovereigntyMode,
) -> Result<Vec<(Vec<u8>, Vec<u8>, u64)>, DbError> {
    with_engine(handle, |engine| {
        engine.authorize_read_namespace(&namespace, read_mode, consistency)?;
        engine.read_range(
            &namespace,
            &start_key,
            &end_key,
            limit,
            consistency,
            requested_ts,
        )
    })
}

pub fn txn_begin(handle: i64) -> Result<u64, DbError> {
    with_engine_mut(handle, |engine| engine.txn_begin())
}

pub fn txn_prepare(handle: i64, txn_id: u64) -> Result<(), DbError> {
    with_engine_mut(handle, |engine| engine.txn_prepare(txn_id))
}

pub fn txn_commit(handle: i64, txn_id: u64) -> Result<(), DbError> {
    let (work_units, logical_shard) =
        with_engine(handle, |engine| engine.txn_pending_work_units(txn_id))?;
    if work_units > MAX_BATCH_OPS {
        return Err(DbError::limit(format!(
            "txn work units {} exceeds {}",
            work_units, MAX_BATCH_OPS
        )));
    }
    let writer = writer_for_shard(handle, logical_shard)?;
    let (tx, rx) = mpsc::channel();
    let message = WriteEnvelopeMessage {
        envelope: WriteEnvelope::TxnCommit { txn_id },
        #[cfg(test)]
        kind: WriteEnvelopeKind::TxnCommit,
        logical_shard,
        ops_hint: work_units,
        bytes_hint: 0,
        oversize_atomic: oversize_atomic_for(true, work_units, 0),
        enqueued_at: Instant::now(),
        response_tx: tx,
    };
    writer.enqueue(message)?;
    let outcome = rx
        .recv()
        .map_err(|_| DbError::io("writer lane dropped completion"))?;
    match outcome? {
        WriteResult::TxnCommitted => Ok(()),
        WriteResult::Version(_) => Err(DbError::io("unexpected version completion for txn")),
    }
}

pub fn txn_abort(handle: i64, txn_id: u64) -> Result<(), DbError> {
    with_engine_mut(handle, |engine| engine.txn_abort(txn_id))
}

pub fn txn_lock_key(
    handle: i64,
    txn_id: u64,
    namespace: Vec<u8>,
    key: Vec<u8>,
) -> Result<(), DbError> {
    with_engine_mut(handle, |engine| {
        engine.txn_lock_key(txn_id, &namespace, &key)
    })
}

pub fn txn_lock_range(
    handle: i64,
    txn_id: u64,
    namespace: Vec<u8>,
    start_key: Vec<u8>,
    end_key: Vec<u8>,
) -> Result<(), DbError> {
    with_engine_mut(handle, |engine| {
        engine.txn_lock_range(txn_id, &namespace, &start_key, &end_key)
    })
}

pub fn membership_set_voters(handle: i64, voters: Vec<u64>) -> Result<(), DbError> {
    with_engine_mut(handle, |engine| engine.set_membership_voters(voters))
}

pub fn membership_begin_joint_change(
    handle: i64,
    change: MembershipChange,
    log_index: u64,
) -> Result<(), DbError> {
    with_engine_mut(handle, |engine| {
        engine.begin_membership_change(change, log_index)
    })
}

pub fn membership_commit_joint_change(handle: i64) -> Result<(), DbError> {
    with_engine_mut(handle, |engine| engine.commit_membership_change())
}

pub fn membership_abort_joint_change(handle: i64) -> Result<(), DbError> {
    with_engine_mut(handle, |engine| engine.abort_membership_change())
}

pub fn snapshot_start(handle: i64) -> Result<u64, DbError> {
    with_engine_mut(handle, |engine| engine.snapshot_start())
}

pub fn snapshot_status(handle: i64, snapshot_id: u64) -> Result<u8, DbError> {
    with_engine(handle, |engine| engine.snapshot_status(snapshot_id))
}

pub fn restore_snapshot(handle: i64, snapshot_id: u64) -> Result<(), DbError> {
    let runtime_state = quiesce_runtime_for_snapshot_restore(handle)?;
    let restore_result = {
        let mut engine = lock_engine_write(&runtime_state.db)?;
        engine.restore_snapshot(snapshot_id)
    };
    match restore_result {
        Ok(()) => restart_runtime_after_snapshot_restore(handle, &runtime_state),
        Err(err) => {
            if let Err(restart_err) = restart_runtime_after_snapshot_restore(handle, &runtime_state)
            {
                return Err(DbError::io(format!(
                    "SNAPSHOT_RESTORE_RECOVERY_FAILED: restore_error={}; restart_error={}",
                    err.message, restart_err.message
                )));
            }
            Err(err)
        }
    }
}

pub fn cdc_page(
    handle: i64,
    after_commit_seq: u64,
    limit: usize,
    shard_filter: Option<Vec<u8>>,
) -> Result<crate::db::cdc::CdcPage, DbError> {
    with_engine(handle, |engine| {
        Ok(engine.cdc_page(after_commit_seq, limit, shard_filter.as_deref()))
    })
}

pub fn cdc_ack(handle: i64, stream: String, commit_seq: u64) -> Result<u64, DbError> {
    if stream.trim().is_empty() {
        return Err(DbError::invalid_argument("cdc stream must be non-empty"));
    }
    with_engine_mut(handle, |engine| engine.cdc_ack(&stream, commit_seq))
}

pub fn cdc_checkpoint(handle: i64, stream: String) -> Result<Option<u64>, DbError> {
    if stream.trim().is_empty() {
        return Err(DbError::invalid_argument("cdc stream must be non-empty"));
    }
    with_engine(handle, |engine| Ok(engine.cdc_checkpoint(&stream)))
}

pub fn safe_time_diagnostics(
    handle: i64,
    budgets: SafeTimeLagBudget,
) -> Result<SafeTimeDiagnostics, DbError> {
    with_engine(handle, |engine| Ok(engine.safe_time_diagnostics(budgets)))
}

pub fn db_health_status(handle: i64) -> Result<DbHealthStatus, DbError> {
    let mut status = with_engine_mut(handle, |engine| Ok(engine.health_status()))?;
    let writer_pool = writer_pool_for_handle(handle)?;
    status.writer_lanes = writer_pool.statuses();
    let total_attempts = status
        .writer_lanes
        .iter()
        .map(|lane| lane.enqueue_attempts)
        .sum::<u64>();
    let max_attempts = status
        .writer_lanes
        .iter()
        .map(|lane| lane.enqueue_attempts)
        .max()
        .unwrap_or(0);
    let to_bps = |num: u64, den: u64| -> u64 {
        if den == 0 {
            0
        } else {
            num.saturating_mul(10_000) / den
        }
    };
    status.writer_lane_max_enqueue_share_bps = to_bps(max_attempts, total_attempts);
    status.writer_lane_max_retry_after_bps = status
        .writer_lanes
        .iter()
        .map(|lane| to_bps(lane.enqueue_rejections, lane.enqueue_attempts))
        .max()
        .unwrap_or(0);
    status.writer_lane_max_saturation_bps = status
        .writer_lanes
        .iter()
        .map(|lane| to_bps(lane.saturated_samples, lane.depth_samples))
        .max()
        .unwrap_or(0);
    let (lookups, hits, misses) = writer_pool.assignment_stats();
    status.writer_lane_assignment_lookups = lookups;
    status.writer_lane_assignment_hits = hits;
    status.writer_lane_assignment_misses = misses;
    status.writer_lane_assignment_hit_rate_bps = to_bps(hits, lookups);
    let apply_pool = apply_lane_pool_for_handle(handle)?;
    status.apply_lanes = apply_pool.statuses();
    status.apply_lane_max_queue_depth = status
        .apply_lanes
        .iter()
        .map(|lane| lane.max_queue_depth)
        .max()
        .unwrap_or(0);
    Ok(status)
}

pub fn db_commit_visibility_status(handle: i64) -> Result<DbCommitVisibilityStatus, DbError> {
    with_engine(handle, |engine| Ok(engine.commit_visibility_status()))
}

pub fn db_write_stage_aggregate(handle: i64) -> Result<DbWriteStageAggregate, DbError> {
    let queue = writer_pool_for_handle(handle)?.telemetry_snapshot();
    with_engine(handle, |engine| Ok(engine.write_stage_aggregate(queue)))
}

pub fn db_client_write_path_aggregate(handle: i64) -> Result<DbClientWritePathAggregate, DbError> {
    with_engine(handle, |engine| Ok(engine.client_write_path_aggregate()))
}

pub fn db_wal_flush_stats(handle: i64) -> Result<DbWalFlushStats, DbError> {
    with_engine(handle, |engine| Ok(engine.wal_flush_stats()))
}

pub fn private_mesh_status(handle: i64) -> Result<DbPrivateMeshStatus, DbError> {
    let _ = db_for_handle(handle)?;
    if let Some(mesh) = mesh_for_handle(handle)? {
        return Ok(mesh.status());
    }
    let machine_id = private_mesh_local_node_id();
    Ok(DbPrivateMeshStatus {
        mesh_ready: true,
        reason: "private mesh disabled".to_string(),
        machine_id: machine_id.clone(),
        leader_id: machine_id.clone(),
        node_count: 1,
        min_ready_nodes: 1,
        nodes: vec![machine_id],
        last_refresh_epoch_ms: now_epoch_ms(),
    })
}

pub fn checkpoint_create(handle: i64) -> Result<crate::db::checkpoint::CheckpointInfo, DbError> {
    with_engine_mut(handle, |engine| engine.checkpoint_create())
}

pub fn checkpoint_restore_latest(
    handle: i64,
) -> Result<crate::db::checkpoint::CheckpointInfo, DbError> {
    with_engine_mut(handle, |engine| engine.checkpoint_restore_latest())
}

pub fn checkpoint_list(handle: i64) -> Result<Vec<crate::db::checkpoint::CheckpointInfo>, DbError> {
    with_engine(handle, |engine| engine.checkpoint_list())
}

pub fn checkpoint_prune(handle: i64, retain: usize) -> Result<(), DbError> {
    with_engine(handle, |engine| engine.checkpoint_prune(retain))
}

pub fn schema_set_committed_epoch(handle: i64, epoch: u64) -> Result<(), DbError> {
    with_engine_mut(handle, |engine| {
        engine.schema_committed_epoch = epoch.max(1);
        Ok(())
    })
}

pub fn schema_committed_epoch(handle: i64) -> Result<u64, DbError> {
    with_engine(handle, |engine| Ok(engine.schema_committed_epoch))
}

pub fn schema_set_all_voters_on_target_binary(handle: i64, ready: bool) -> Result<(), DbError> {
    with_engine_mut(handle, |engine| {
        engine.schema_all_voters_on_target_binary = ready;
        Ok(())
    })
}

pub fn logical_shard_count(handle: i64) -> Result<u32, DbError> {
    with_engine(handle, |engine| Ok(engine.logical_shard_count()))
}

pub fn active_group_count(handle: i64) -> Result<u32, DbError> {
    with_engine(handle, |engine| Ok(engine.active_group_count()))
}

pub fn topology_status(handle: i64) -> Result<DbTopologyStatus, DbError> {
    with_engine(handle, |engine| Ok(engine.topology_status()))
}

pub fn autoscale_status(handle: i64) -> Result<DbAutoscaleStatus, DbError> {
    with_engine(handle, |engine| Ok(engine.autoscale_status()))
}

pub fn intent_effective(handle: i64) -> Result<DbIntentEffective, DbError> {
    with_engine(handle, |engine| Ok(engine.intent_effective()))
}

pub fn intent_conflicts(handle: i64) -> Result<Vec<DbIntentConflict>, DbError> {
    with_engine(handle, |engine| Ok(engine.intent_conflicts()))
}

pub fn autopilot_last_actions(
    handle: i64,
    limit: usize,
) -> Result<Vec<DbAutopilotAuditRow>, DbError> {
    with_engine(handle, |engine| Ok(engine.autopilot_last_actions(limit)))
}

pub fn tiering_state(handle: i64) -> Result<DbTieringState, DbError> {
    with_engine(handle, |engine| Ok(engine.tiering_state()))
}

pub fn recommendations(handle: i64) -> Result<Vec<DbRecommendation>, DbError> {
    with_engine(handle, |engine| Ok(engine.recommendations()))
}

pub fn autoscale_tick(handle: i64) -> Result<DbAutoscaleStatus, DbError> {
    with_engine_mut(handle, |engine| engine.autoscale_tick())
}

pub fn shard_map_epoch(handle: i64) -> Result<u64, DbError> {
    with_engine(handle, |engine| Ok(engine.shard_map_epoch()))
}

pub fn shard_for_key(handle: i64, namespace: Vec<u8>, key: Vec<u8>) -> Result<u32, DbError> {
    with_engine(handle, |engine| {
        engine.route_namespace_key(&namespace, &key)
    })
}

pub fn resolve_owner(
    handle: i64,
    namespace: Vec<u8>,
    key: Vec<u8>,
) -> Result<OwnerRecord, DbError> {
    with_engine_mut(handle, |engine| engine.resolve_owner(&namespace, &key))
}

pub fn global_route_lookup(
    handle: i64,
    namespace: Vec<u8>,
    key: Vec<u8>,
) -> Result<OwnerRecord, DbError> {
    resolve_owner(handle, namespace, key)
}

pub fn split_logical_shard(handle: i64, shard_id: u32) -> Result<(u32, u32), DbError> {
    with_engine_mut(handle, |engine| engine.split_logical_shard(shard_id))
}

pub fn merge_logical_shards(
    handle: i64,
    left_shard_id: u32,
    right_shard_id: u32,
) -> Result<u32, DbError> {
    with_engine_mut(handle, |engine| {
        engine.merge_logical_shards(left_shard_id, right_shard_id)
    })
}

pub fn plan_home_relocation(
    handle: i64,
    keyrange_id: String,
    target_region: String,
    reason: String,
) -> Result<crate::db::placement::RelocationJob, DbError> {
    with_engine_mut(handle, |engine| {
        engine.plan_home_relocation(&keyrange_id, &target_region, &reason)
    })
}

pub fn advance_home_relocation(
    handle: i64,
    job_id: String,
    phase_ack: Option<crate::db::placement::RelocationPhase>,
) -> Result<crate::db::placement::RelocationJob, DbError> {
    with_engine_mut(handle, |engine| {
        engine.advance_home_relocation(&job_id, phase_ack)
    })
}

pub fn promote_async_failover(
    handle: i64,
    keyrange_id: String,
    region: String,
    expected_epoch: u64,
) -> Result<OwnerRecord, DbError> {
    with_engine_mut(handle, |engine| {
        engine.promote_async_failover(&keyrange_id, &region, expected_epoch)
    })
}

pub fn cdc_stream_page(
    handle: i64,
    stream: String,
    limit: usize,
    shard_filter: Option<Vec<u8>>,
) -> Result<crate::db::cdc::CdcPage, DbError> {
    if stream.trim().is_empty() {
        return Err(DbError::invalid_argument("cdc stream must be non-empty"));
    }
    let after_commit_seq = cdc_checkpoint(handle, stream)?.unwrap_or(0);
    cdc_page(handle, after_commit_seq, limit, shard_filter)
}

pub fn cdc_stream_backfill_page(
    handle: i64,
    stream: String,
    backfill_start_inclusive: u64,
    limit: usize,
    shard_filter: Option<Vec<u8>>,
) -> Result<crate::db::cdc::CdcPage, DbError> {
    if stream.trim().is_empty() {
        return Err(DbError::invalid_argument("cdc stream must be non-empty"));
    }
    let checkpoint = cdc_checkpoint(handle, stream)?;
    let after_commit_seq = checkpoint.unwrap_or_else(|| backfill_start_inclusive.saturating_sub(1));
    cdc_page(handle, after_commit_seq, limit, shard_filter)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::cdc::CdcOpKind;
    use crate::db::raft::message::AppendEntriesResponse;
    use crate::db::replication::quorum::FollowerAppendResponse;
    use crate::db::security::residency::{
        ReadSovereigntyCap, ReadSovereigntyMode, ResidencyPolicy, ResidencyRule,
    };
    use crate::db::types::ErrorCode;
    use crate::db::wal::format::{Record, RecordKind, encode};
    use crate::db::wal::segment::WalAppendMetrics;
    use crate::db::writer::DetachedWriterQueue;
    use crate::kernel::actor::{actor_send, actor_spawn, pending_await, pool_new, register_method};
    use crate::list::{list_new, list_push};
    use crate::result::result_unwrap;
    use crate::string::{str_from_utf8, with_string_bytes};
    use crate::value::int_value;
    use crate::{Value, wr_rc_dec};
    use crossbeam_channel::bounded;
    use std::collections::{BTreeMap, BTreeSet, HashMap};
    use std::fs::OpenOptions;
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::path::Path;
    use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

    #[test]
    fn db_registry_handle_allocator_stops_at_max() {
        let next = AtomicI64::new(i64::MAX - 1);
        assert_eq!(next_positive_i64_handle(&next), Some(i64::MAX - 1));
        assert_eq!(next_positive_i64_handle(&next), Some(i64::MAX));
        assert_eq!(next_positive_i64_handle(&next), None);
    }

    #[test]
    fn wal_completion_polling_preserves_partial_ready_results() {
        let (tx_a, rx_a) = bounded(1);
        let (tx_b, rx_b) = bounded(1);
        let rx_list = vec![rx_a, rx_b];
        let mut completion_results: Vec<Option<io::Result<WalBatchCompletion>>> = vec![None, None];

        tx_a.send(Ok(WalBatchCompletion {
            offset: 11,
            metrics: WalAppendMetrics::default(),
            completed_at: Instant::now(),
        }))
        .expect("send completion a");

        assert!(
            !poll_wal_completion_receivers(&rx_list, &mut completion_results),
            "single ready completion must not mark group complete"
        );
        assert!(completion_results[0].is_some());
        assert!(completion_results[1].is_none());

        tx_b.send(Ok(WalBatchCompletion {
            offset: 22,
            metrics: WalAppendMetrics::default(),
            completed_at: Instant::now(),
        }))
        .expect("send completion b");

        assert!(
            poll_wal_completion_receivers(&rx_list, &mut completion_results),
            "all completions ready should complete without losing earlier result"
        );

        let drained = drain_wal_completions_blocking(
            &rx_list,
            &mut completion_results,
            Instant::now(),
            Duration::from_secs(1),
        );
        assert_eq!(drained.len(), 2);
        assert!(drained.iter().all(Result::is_ok));
        let offsets = drained
            .into_iter()
            .map(|result| result.expect("completion").offset)
            .collect::<Vec<_>>();
        assert_eq!(offsets, vec![11, 22]);
    }

    fn test_discovery_config() -> PrivateMeshDiscoveryConfig {
        PrivateMeshDiscoveryConfig {
            app_name: "demo-app".to_string(),
            private_rpc_port: 19_091,
            refresh_interval: Duration::from_secs(5),
            lookup_timeout: Duration::from_secs(1),
            leader_override: None,
        }
    }

    #[test]
    fn mesh_snapshot_prefers_minimum_leader_then_fails_over_lexicographically() {
        let discovery = test_discovery_config();
        let elected = mesh_snapshot_from_nodes(
            "m-1",
            vec!["m-1".to_string(), "m-2".to_string(), "m-3".to_string()],
            &discovery,
        )
        .expect("elected snapshot");
        assert_eq!(elected.leader_node_id, "m-1");

        let failover = mesh_snapshot_from_nodes(
            "m-1",
            vec!["m-1".to_string(), "m-3".to_string()],
            &discovery,
        )
        .expect("failover snapshot");
        assert_eq!(failover.leader_node_id, "m-1");
    }

    #[test]
    fn mesh_snapshot_injects_local_machine_when_dns_misses_local() {
        let discovery = test_discovery_config();
        let snapshot = mesh_snapshot_from_nodes(
            "m-1",
            vec!["m-2".to_string(), "m-3".to_string()],
            &discovery,
        )
        .expect("snapshot");
        assert!(snapshot.cluster_nodes.contains(&"m-1".to_string()));
        assert_eq!(
            snapshot.addresses.get("m-1").map(String::as_str),
            Some("127.0.0.1:19091")
        );
    }

    #[test]
    fn mesh_snapshot_bootstrap_from_local_fallback_elects_smallest_discovered_leader() {
        let discovery = test_discovery_config();
        let snapshot = mesh_snapshot_from_nodes(
            "m-3",
            vec!["m-1".to_string(), "m-2".to_string(), "m-3".to_string()],
            &discovery,
        )
        .expect("snapshot");
        assert_eq!(snapshot.leader_node_id, "m-1");
    }

    #[test]
    fn mesh_readiness_requires_minimum_nodes_in_strict_mode() {
        let snapshot = PrivateMeshSnapshot {
            leader_node_id: "m-1".to_string(),
            cluster_nodes: vec!["m-1".to_string(), "m-2".to_string()],
            addresses: HashMap::new(),
        };
        let readiness = mesh_readiness_from_snapshot(&snapshot, true, 3, "test");
        assert!(!readiness.ready);
        assert!(readiness.reason.contains("waiting for nodes 2/3"));

        let ready_snapshot = PrivateMeshSnapshot {
            leader_node_id: "m-1".to_string(),
            cluster_nodes: vec!["m-1".to_string(), "m-2".to_string(), "m-3".to_string()],
            addresses: HashMap::new(),
        };
        let ready = mesh_readiness_from_snapshot(&ready_snapshot, true, 3, "test");
        assert!(ready.ready);
        assert!(ready.reason.contains("discovered 3/3"));
    }

    #[test]
    fn private_rpc_min_ready_nodes_derives_from_target_voters() {
        assert_eq!(
            private_rpc_min_ready_nodes_from(Some("5"), None),
            5,
            "target voters should drive default min-ready"
        );
        assert_eq!(
            private_rpc_min_ready_nodes_from(Some("5"), Some("3")),
            3,
            "explicit min-ready should override derived target voters"
        );
        assert_eq!(
            private_rpc_min_ready_nodes_from(None, None),
            PRIVATE_RPC_MIN_READY_NODES_DEFAULT
        );
    }

    #[test]
    fn private_rpc_bind_addr_defaults_to_ipv6_on_fly() {
        assert_eq!(
            default_private_rpc_bind_addr(19_091, Some("demo-app")),
            "[::]:19091"
        );
        assert_eq!(default_private_rpc_bind_addr(19_091, None), "0.0.0.0:19091");
    }

    fn temp_dir() -> PathBuf {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let base = std::env::temp_dir().join(format!(
            "wrela_db_test_{}_{}_{}",
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

    fn writer_groups_for_handle(handle: i64) -> Vec<WriterCommittedGroup> {
        registry()
            .writers
            .lock()
            .expect("writer registry lock")
            .get(&handle)
            .expect("writer handle")
            .committed_groups()
    }

    fn collapsed_topology_map_for(region: &str) -> BTreeMap<String, BTreeMap<String, Vec<String>>> {
        let normalized = normalize_region_id(region).unwrap_or_else(|| "local".to_string());
        BTreeMap::from([(
            normalized.clone(),
            BTreeMap::from([(normalized.clone(), vec![format!("{normalized}-node-1")])]),
        )])
    }

    fn open_with_test_options(
        data_dir: &Path,
        local_region: &str,
        initial_logical_shards: u32,
        residency_policy: Option<ResidencyPolicy>,
        checkpoint_allowed_regions: Vec<&str>,
    ) -> Result<i64, DbError> {
        let mut checkpoint = crate::db::checkpoint::CheckpointConfig::default();
        checkpoint.allowed_regions = checkpoint_allowed_regions
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        let config = DbConfig::for_testing()
            .with_checkpoint(checkpoint)
            .with_replication(config::ReplicationConfig {
                factor: 3,
                write_quorum: 2,
                log_backend: ReplicatedLogBackend::DualWal,
                ..DbConfig::for_testing().replication
            })
            .with_topology(config::TopologyConfig {
                initial_logical_shards,
                initial_active_groups: 3,
                local_region: local_region.to_string(),
                region_az_node_map: collapsed_topology_map_for(local_region),
                residency_policy: residency_policy.clone(),
                ..Default::default()
            });
        let config = DbConfig {
            residency_policy,
            ..config
        };
        open_db_with_config(data_dir, &config)
    }

    fn open_with_backend(data_dir: &Path, backend: ReplicatedLogBackend) -> Result<i64, DbError> {
        let config = DbConfig::for_testing().with_replication(config::ReplicationConfig {
            factor: 1,
            write_quorum: 1,
            log_backend: backend,
            ..DbConfig::for_testing().replication
        });
        open_db_with_config(data_dir, &config)
    }

    fn str_value(input: &str) -> Value {
        str_from_utf8(input.as_ptr(), input.len())
    }

    #[test]
    fn commit_visibility_status_reports_backlog_depth() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let baseline = db_commit_visibility_status(handle).expect("visibility status");
        assert_eq!(baseline.apply_backlog_depth, 0);
        {
            let db = db_for_handle(handle).expect("db handle");
            let mut engine = db.write().expect("DB engine lock");
            let replication = engine.primary_replication_mut();
            replication.durability_commit_index = 12;
            replication.apply_visible_index = 9;
        }
        let status = db_commit_visibility_status(handle).expect("visibility status");
        assert_eq!(status.durability_commit_index, 12);
        assert_eq!(status.apply_visible_index, 9);
        assert_eq!(status.apply_backlog_depth, 3);
        assert!(close_db(handle));
    }

    #[test]
    fn compaction_scheduler_uses_current_backlog_not_peak_backlog() {
        let dir = temp_dir();
        let handle = open_with_backend(&dir, ReplicatedLogBackend::DualWal).expect("open db");
        {
            let db = db_for_handle(handle).expect("db handle");
            let mut engine = db.write().expect("DB engine lock");
            engine.apply_backlog_peak = 9_999;
            {
                let primary = engine.primary_replication_mut();
                primary.durability_commit_index = 42;
                primary.apply_visible_index = 42;
            }
            engine.lsm_cached_stats.compaction_debt_bytes_estimate =
                compaction_scheduler_max_debt_bytes_default().saturating_add(1);
            engine.lsm_stats_dirty = false;
            let decision = engine.compaction_admission_decision_for_group(PRIMARY_ACTIVE_GROUP_ID);
            assert_eq!(
                decision,
                crate::db::lsm::scheduler::CompactionAdmissionDecision::Admit
            );
        }
        assert!(close_db(handle));
    }

    #[test]
    fn async_apply_mode_blocks_strong_reads_until_apply_visible() {
        let dir = temp_dir();
        let config = DbConfig::for_testing().with_replication(config::ReplicationConfig {
            commit_visibility_mode: CommitVisibilityMode::AsyncApply,
            ..DbConfig::for_testing().replication
        });
        let handle = open_db_with_config(&dir, &config).expect("open db");
        submit_put(
            handle,
            b"core".to_vec(),
            b"async-k".to_vec(),
            b"async-v".to_vec(),
            None,
        )
        .expect("put");
        {
            let db = db_for_handle(handle).expect("db handle");
            let mut engine = db.write().expect("DB engine lock");
            let replication = engine.primary_replication_mut();
            replication.durability_commit_index = replication
                .durability_commit_index
                .max(replication.apply_visible_index.saturating_add(1));
        }
        let err = read_point(handle, b"core".to_vec(), b"async-k".to_vec())
            .expect_err("strong read should block while async apply backlog exists");
        assert_eq!(err.code, ErrorCode::LimitExceeded);
        assert!(err.message.contains("STRONG_READ_APPLY_BACKLOG"));
        {
            let db = db_for_handle(handle).expect("db handle");
            let mut engine = db.write().expect("DB engine lock");
            let replication = engine.primary_replication_mut();
            replication.apply_visible_index = replication.durability_commit_index;
        }
        let value = read_point(handle, b"core".to_vec(), b"async-k".to_vec())
            .expect("read")
            .expect("value");
        assert_eq!(value, b"async-v".to_vec());
        assert!(close_db(handle));
    }

    #[test]
    fn put_get_roundtrip() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let version = submit_put(
            handle,
            b"core".to_vec(),
            b"k1".to_vec(),
            b"v1".to_vec(),
            None,
        )
        .expect("put");
        assert!(version >= 1);
        let value = read_point(handle, b"core".to_vec(), b"k1".to_vec())
            .expect("read")
            .expect("value");
        assert_eq!(value, b"v1".to_vec());
        assert!(close_db(handle));
    }

    #[test]
    fn point_read_cache_counts_hit_after_warm() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        submit_put(
            handle,
            b"core".to_vec(),
            b"cache-k".to_vec(),
            b"cache-v".to_vec(),
            None,
        )
        .expect("put");
        assert_eq!(
            read_point(handle, b"core".to_vec(), b"cache-k".to_vec()).expect("first read"),
            Some(b"cache-v".to_vec())
        );
        assert_eq!(
            read_point(handle, b"core".to_vec(), b"cache-k".to_vec()).expect("second read"),
            Some(b"cache-v".to_vec())
        );

        let db = registry()
            .handles
            .lock()
            .expect("DB registry lock")
            .get(&handle)
            .cloned()
            .expect("db handle");
        let engine = db.write().expect("DB engine lock");
        let stats = engine.read_stats();
        assert_eq!(stats.point_cache_misses, 1);
        assert_eq!(stats.point_cache_hits, 1);
        assert_eq!(stats.negative_shortcuts, 0);
        drop(engine);
        assert!(close_db(handle));
    }

    #[test]
    fn timestamped_read_bypasses_latest_point_cache_entry() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let v1 = submit_put(
            handle,
            b"core".to_vec(),
            b"hist-cache-k".to_vec(),
            b"v1".to_vec(),
            None,
        )
        .expect("put v1");
        submit_put(
            handle,
            b"core".to_vec(),
            b"hist-cache-k".to_vec(),
            b"v2".to_vec(),
            None,
        )
        .expect("put v2");
        assert_eq!(
            read_point(handle, b"core".to_vec(), b"hist-cache-k".to_vec()).expect("latest read"),
            Some(b"v2".to_vec())
        );

        let historical = read_point_consistent(
            handle,
            b"core".to_vec(),
            b"hist-cache-k".to_vec(),
            ReadConsistency::Strong,
            Some(v1),
        )
        .expect("historical read");
        assert_eq!(historical, Some(b"v1".to_vec()));
        assert!(close_db(handle));
    }

    #[test]
    fn timestamped_read_bypasses_negative_shortcut() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let v1 = submit_put(
            handle,
            b"core".to_vec(),
            b"hist-neg-k".to_vec(),
            b"v1".to_vec(),
            None,
        )
        .expect("put v1");
        submit_batch(
            handle,
            &[BatchOp::Delete {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"hist-neg-k"),
                expected_version: None,
            }],
        )
        .expect("delete latest");
        assert_eq!(
            read_point(handle, b"core".to_vec(), b"hist-neg-k".to_vec())
                .expect("latest tombstone read"),
            None
        );

        let historical = read_point_consistent(
            handle,
            b"core".to_vec(),
            b"hist-neg-k".to_vec(),
            ReadConsistency::Strong,
            Some(v1),
        )
        .expect("historical read");
        assert_eq!(historical, Some(b"v1".to_vec()));
        assert!(close_db(handle));
    }

    #[test]
    fn range_iterator_supports_coarse_cancellation() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        submit_put(handle, b"core".to_vec(), b"a".to_vec(), b"1".to_vec(), None).expect("put a");
        submit_put(handle, b"core".to_vec(), b"b".to_vec(), b"2".to_vec(), None).expect("put b");

        let db = registry()
            .handles
            .lock()
            .expect("DB registry lock")
            .get(&handle)
            .cloned()
            .expect("db handle");
        let cancel = RangeCancellation::new();
        let mut iter = {
            let engine = db.write().expect("DB engine lock");
            engine
                .read_range_iter(
                    b"core",
                    b"a",
                    b"z",
                    10,
                    cancel.clone(),
                    ReadConsistency::Strong,
                    None,
                )
                .expect("range iterator")
        };
        assert!(iter.try_next().expect("first row").is_some());
        cancel.cancel();
        let err = iter
            .try_next()
            .expect_err("cancelled iterator should reject");
        assert_eq!(err.code, ErrorCode::LimitExceeded);
        assert!(err.message.contains("RETRY_AFTER_MS=25"));
        assert!(close_db(handle));
    }

    #[test]
    fn wal_recovery_replays_data() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        submit_put(
            handle,
            b"core".to_vec(),
            b"k2".to_vec(),
            b"v2".to_vec(),
            None,
        )
        .expect("put");
        assert!(close_db(handle));

        let handle2 = open_db(&dir).expect("reopen db");
        let value = read_point(handle2, b"core".to_vec(), b"k2".to_vec())
            .expect("read")
            .expect("value");
        assert_eq!(value, b"v2".to_vec());
        assert!(close_db(handle2));
    }

    #[test]
    fn key_encode_decode_roundtrip_and_malformed_rejection() {
        let encoded = crate::db::keyspace::encode_user_key(b"core", b"k1").expect("encode");
        let (ns, key) = crate::db::keyspace::decode_user_key(&encoded).expect("decode");
        assert_eq!(ns, b"core".to_vec());
        assert_eq!(key, b"k1".to_vec());

        let malformed = vec![0u8, 10u8, 1u8];
        let err = crate::db::keyspace::decode_user_key(&malformed).expect_err("malformed decode");
        assert_eq!(err.code, ErrorCode::InvalidArgument);
    }

    #[test]
    fn limit_checks_reject_oversized_key_and_value() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");

        let key_err = submit_put(
            handle,
            b"core".to_vec(),
            vec![b'k'; MAX_KEY_BYTES + 1],
            b"v".to_vec(),
            None,
        )
        .expect_err("oversized key should fail");
        assert_eq!(key_err.code, ErrorCode::LimitExceeded);

        let value_err = submit_put(
            handle,
            b"core".to_vec(),
            b"k".to_vec(),
            vec![b'v'; MAX_VALUE_BYTES + 1],
            None,
        )
        .expect_err("oversized value should fail");
        assert_eq!(value_err.code, ErrorCode::LimitExceeded);

        assert!(close_db(handle));
    }

    #[test]
    fn namespace_isolation_and_occ_mismatch_are_deterministic() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");

        let version = submit_put(
            handle,
            b"core".to_vec(),
            b"k".to_vec(),
            b"v1".to_vec(),
            None,
        )
        .expect("put core");
        submit_put(
            handle,
            b"tenant2".to_vec(),
            b"k".to_vec(),
            b"v2".to_vec(),
            None,
        )
        .expect("put tenant2");

        let core = read_point(handle, b"core".to_vec(), b"k".to_vec())
            .expect("read core")
            .expect("core value");
        let t2 = read_point(handle, b"tenant2".to_vec(), b"k".to_vec())
            .expect("read tenant2")
            .expect("tenant2 value");
        assert_eq!(core, b"v1".to_vec());
        assert_eq!(t2, b"v2".to_vec());

        let occ_err = submit_put(
            handle,
            b"core".to_vec(),
            b"k".to_vec(),
            b"v3".to_vec(),
            Some(version.saturating_add(99)),
        )
        .expect_err("occ mismatch expected");
        assert_eq!(occ_err.code, ErrorCode::OccMismatch);

        assert!(close_db(handle));
    }

    #[test]
    fn wal_recovery_truncates_torn_tail_record() {
        let dir = temp_dir();
        let wal_path = wal_path_from(&dir);
        let handle = open_db(&dir).expect("open db");
        submit_put(
            handle,
            b"core".to_vec(),
            b"persisted".to_vec(),
            b"ok".to_vec(),
            None,
        )
        .expect("put");
        assert!(close_db(handle));

        let partial = encode(&Record {
            kind: RecordKind::Put,
            namespace: Bytes::from_static(b"core"),
            key: Bytes::from_static(b"partial"),
            value: Bytes::from_static(b"bad"),
            version: 999,
        });
        let mut file = OpenOptions::new()
            .append(true)
            .open(&wal_path)
            .expect("open wal append");
        file.write_all(&partial[..partial.len() / 2])
            .expect("append partial");
        file.sync_data().expect("sync");

        let reopened = open_db(&dir).expect("reopen should ignore torn tail");
        let persisted = read_point(reopened, b"core".to_vec(), b"persisted".to_vec())
            .expect("read persisted")
            .expect("persisted exists");
        let partial_read =
            read_point(reopened, b"core".to_vec(), b"partial".to_vec()).expect("read partial");
        assert_eq!(persisted, b"ok".to_vec());
        assert!(partial_read.is_none());
        assert!(close_db(reopened));
    }

    #[test]
    fn replica_wal_direct_rejects_payload_ops_mismatch() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let owner =
            resolve_owner(handle, b"core".to_vec(), b"k-mismatch".to_vec()).expect("resolve owner");
        let fence = OwnershipFence {
            expected_home_epoch: owner.home_epoch,
            expected_shard_map_epoch: owner.shard_map_epoch,
            ownership_token: owner.ownership_token,
        };
        let ops = vec![BatchOp::Put {
            namespace: Bytes::from_static(b"core"),
            key: Bytes::from_static(b"k-mismatch"),
            value: Bytes::from_static(b"expected"),
            expected_version: None,
        }];
        let wal_bytes = encode(&Record {
            kind: RecordKind::Put,
            namespace: Bytes::from_static(b"core"),
            key: Bytes::from_static(b"k-mismatch"),
            value: Bytes::from_static(b"different"),
            version: 11,
        });
        let err = submit_replica_wal_direct_with_ownership_fence(handle, &wal_bytes, &ops, fence)
            .expect_err("mismatched wal payload must fail");
        assert_eq!(err.code, ErrorCode::InvalidArgument);
        assert!(err.message.contains("REPLICA_WAL_PAYLOAD_OP_MISMATCH"));
        let observed = read_point(handle, b"core".to_vec(), b"k-mismatch".to_vec()).expect("read");
        assert!(observed.is_none(), "mismatched payload must not apply");
        assert!(close_db(handle));
    }

    #[test]
    fn replica_wal_direct_rejects_truncated_payload() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let owner =
            resolve_owner(handle, b"core".to_vec(), b"k-trunc".to_vec()).expect("resolve owner");
        let fence = OwnershipFence {
            expected_home_epoch: owner.home_epoch,
            expected_shard_map_epoch: owner.shard_map_epoch,
            ownership_token: owner.ownership_token,
        };
        let ops = vec![BatchOp::Put {
            namespace: Bytes::from_static(b"core"),
            key: Bytes::from_static(b"k-trunc"),
            value: Bytes::from_static(b"value"),
            expected_version: None,
        }];
        let mut wal_bytes = encode(&Record {
            kind: RecordKind::Put,
            namespace: Bytes::from_static(b"core"),
            key: Bytes::from_static(b"k-trunc"),
            value: Bytes::from_static(b"value"),
            version: 12,
        });
        wal_bytes.pop();
        let err = submit_replica_wal_direct_with_ownership_fence(handle, &wal_bytes, &ops, fence)
            .expect_err("truncated wal payload must fail");
        assert_eq!(err.code, ErrorCode::InvalidArgument);
        assert!(err.message.contains("REPLICA_WAL_PAYLOAD_TRUNCATED"));
        let observed = read_point(handle, b"core".to_vec(), b"k-trunc".to_vec()).expect("read");
        assert!(observed.is_none(), "truncated payload must not apply");
        assert!(close_db(handle));
    }

    #[test]
    fn wal_checksum_mismatch_surfaces_io_error_on_open() {
        let dir = temp_dir();
        let wal_path = wal_path_from(&dir);
        let handle = open_db(&dir).expect("open db");
        submit_put(
            handle,
            b"core".to_vec(),
            b"ck".to_vec(),
            b"value".to_vec(),
            None,
        )
        .expect("put");
        assert!(close_db(handle));

        let mut bytes = Vec::new();
        let mut reader = OpenOptions::new()
            .read(true)
            .open(&wal_path)
            .expect("open wal");
        reader.read_to_end(&mut bytes).expect("read wal");
        let last = bytes.len().saturating_sub(1);
        bytes[last] ^= 0xFF;

        let mut writer = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&wal_path)
            .expect("reopen wal");
        writer.write_all(&bytes).expect("rewrite wal");
        writer.flush().expect("flush wal");
        writer.seek(SeekFrom::Start(0)).expect("seek");

        let err = open_db(&dir).expect_err("checksum mismatch should fail open");
        assert_eq!(err.code, ErrorCode::Io);
    }

    #[test]
    fn detached_writer_queue_applies_retry_after_backpressure() {
        let mut q = DetachedWriterQueue::<BatchOp>::new(1);
        q.push(BatchOp::Put {
            namespace: Bytes::from_static(b"core"),
            key: Bytes::from_static(b"k1"),
            value: Bytes::from_static(b"v1"),
            expected_version: None,
        })
        .expect("first enqueue");
        let err = q
            .push(BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"k2"),
                value: Bytes::from_static(b"v2"),
                expected_version: None,
            })
            .expect_err("queue should backpressure");
        assert_eq!(err.code, ErrorCode::LimitExceeded);
        assert!(err.message.contains("RETRY_AFTER_MS=25"));
    }

    #[test]
    fn oversize_client_batch_commits_as_single_atomic_group() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let mut batch = Vec::new();
        for idx in 0..(DEFAULT_WRITE_FLUSH_MAX_OPS + 32) {
            batch.push(BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: format!("k-{idx:04}").into_bytes().into(),
                value: Bytes::from_static(b"v"),
                expected_version: None,
            });
        }
        submit_batch(handle, &batch).expect("oversize batch");
        let groups = writer_groups_for_handle(handle);
        assert!(
            groups
                .iter()
                .any(|group| group.kinds == vec![WriteEnvelopeKind::ClientBatch]),
            "oversize client batch should commit as its own group"
        );
        assert!(close_db(handle));
    }

    #[test]
    fn oversize_txn_commit_commits_as_single_atomic_group() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let txn_id = txn_begin(handle).expect("txn begin");
        for idx in 0..(DEFAULT_WRITE_FLUSH_MAX_OPS + 32) {
            txn_lock_key(
                handle,
                txn_id,
                b"core".to_vec(),
                format!("lock-{idx:04}").into_bytes(),
            )
            .expect("lock key");
        }
        txn_commit(handle, txn_id).expect("txn commit");
        let groups = writer_groups_for_handle(handle);
        assert!(
            groups
                .iter()
                .any(|group| group.kinds == vec![WriteEnvelopeKind::TxnCommit]),
            "oversize txn commit should commit as its own group"
        );
        assert!(close_db(handle));
    }

    #[test]
    fn oversize_atomic_envelope_not_interleaved_with_neighbor_puts() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let h1 = std::thread::spawn({
            move || {
                submit_put(
                    handle,
                    b"core".to_vec(),
                    b"before".to_vec(),
                    b"v1".to_vec(),
                    None,
                )
                .expect("put before");
            }
        });
        std::thread::sleep(Duration::from_millis(1));
        let mut oversize = Vec::new();
        for idx in 0..(DEFAULT_WRITE_FLUSH_MAX_OPS + 16) {
            oversize.push(BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: format!("mid-{idx:04}").into_bytes().into(),
                value: Bytes::from_static(b"x"),
                expected_version: None,
            });
        }
        submit_batch(handle, &oversize).expect("oversize middle batch");
        submit_put(
            handle,
            b"core".to_vec(),
            b"after".to_vec(),
            b"v2".to_vec(),
            None,
        )
        .expect("put after");
        h1.join().expect("join put before");
        let groups = writer_groups_for_handle(handle);
        let middle = groups
            .iter()
            .position(|group| group.kinds == vec![WriteEnvelopeKind::ClientBatch])
            .expect("middle oversize group present");
        assert!(
            middle > 0 && middle + 1 < groups.len(),
            "oversize group should not consume neighboring puts"
        );
        assert!(close_db(handle));
    }

    #[test]
    fn envelope_exceeding_hard_limits_is_rejected_without_side_effects() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let mut oversized = Vec::new();
        for idx in 0..(MAX_BATCH_OPS + 1) {
            oversized.push(BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: format!("too-many-{idx:05}").into_bytes().into(),
                value: Bytes::from_static(b"v"),
                expected_version: None,
            });
        }
        let err = submit_batch(handle, &oversized).expect_err("oversized batch must fail");
        assert_eq!(err.code, ErrorCode::LimitExceeded);
        let before = read_point(handle, b"core".to_vec(), b"too-many-00000".to_vec())
            .expect("read after reject");
        assert!(
            before.is_none(),
            "rejected oversized envelope must not mutate state"
        );
        assert!(close_db(handle));
    }

    #[test]
    fn mixed_queue_with_oversize_envelope_preserves_group_ordering() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let put_handle = handle;
        let first = std::thread::spawn(move || {
            submit_put(
                put_handle,
                b"core".to_vec(),
                b"order-a".to_vec(),
                b"a".to_vec(),
                None,
            )
            .expect("first put")
        });
        std::thread::sleep(Duration::from_millis(1));
        let mut oversize = Vec::new();
        for idx in 0..(DEFAULT_WRITE_FLUSH_MAX_OPS + 8) {
            oversize.push(BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: format!("order-mid-{idx:04}").into_bytes().into(),
                value: Bytes::from_static(b"m"),
                expected_version: None,
            });
        }
        let mid_version = submit_batch(handle, &oversize).expect("mid batch");
        let tail_version = submit_put(
            handle,
            b"core".to_vec(),
            b"order-z".to_vec(),
            b"z".to_vec(),
            None,
        )
        .expect("tail put");
        let head_version = first.join().expect("join head");
        assert!(head_version < mid_version);
        assert!(mid_version <= tail_version);
        let groups = writer_groups_for_handle(handle);
        let kinds: Vec<Vec<WriteEnvelopeKind>> = groups.into_iter().map(|g| g.kinds).collect();
        let mid = kinds
            .iter()
            .position(|k| k == &vec![WriteEnvelopeKind::ClientBatch])
            .expect("middle batch group");
        assert!(mid > 0);
        assert!(mid + 1 < kinds.len());
        assert!(close_db(handle));
    }

    #[test]
    fn detached_pool_writes_use_writer_lane_and_preserve_oversize_atomicity() {
        const CLASS_ID: u32 = 92_041;

        extern "C" fn detached_db_put(argc: usize, argv: *const Value) -> Value {
            if argc < 5 || argv.is_null() {
                return Value::nil();
            }
            let (handle, namespace, key, value) = unsafe {
                let handle = int_value(*argv.add(1)).unwrap_or(0);
                let namespace = with_string_bytes(*argv.add(2), |bytes| bytes.to_vec());
                let key = with_string_bytes(*argv.add(3), |bytes| bytes.to_vec());
                let value = with_string_bytes(*argv.add(4), |bytes| bytes.to_vec());
                (handle, namespace, key, value)
            };
            let (Some(namespace), Some(key), Some(value)) = (namespace, key, value) else {
                return Value::nil();
            };
            match submit_put(handle, namespace, key, value, None) {
                Ok(version) => Value::from_int(version as i64),
                Err(_) => Value::nil(),
            }
        }

        extern "C" fn detached_oversize_batch(argc: usize, argv: *const Value) -> Value {
            if argc < 2 || argv.is_null() {
                return Value::nil();
            }
            let handle = unsafe { int_value(*argv.add(1)).unwrap_or(0) };
            let mut batch = Vec::new();
            for idx in 0..(DEFAULT_WRITE_FLUSH_MAX_OPS + 32) {
                batch.push(BatchOp::Put {
                    namespace: Bytes::from_static(b"core"),
                    key: format!("detached-mid-{idx:04}").into_bytes().into(),
                    value: Bytes::from_static(b"x"),
                    expected_version: None,
                });
            }
            match submit_batch(handle, &batch) {
                Ok(version) => Value::from_int(version as i64),
                Err(_) => Value::nil(),
            }
        }

        register_method(CLASS_ID, 0, detached_db_put);
        register_method(CLASS_ID, 1, detached_oversize_batch);

        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let actor = actor_spawn(CLASS_ID as u64, Value::nil(), 1, 3, 256, 10, 64);
        let handles = list_new(0);
        list_push(handles, actor);
        let pool = pool_new(handles, 0, 0, 0, 0, 256);

        let before_ns = str_value("core");
        let before_key = str_value("detached-before");
        let before_val = str_value("v1");
        let before_args = [Value::from_int(handle), before_ns, before_key, before_val];
        let before_pending = actor_send(pool, 0, before_args.len(), before_args.as_ptr());

        let oversize_args = [Value::from_int(handle)];
        let oversize_pending = actor_send(pool, 1, oversize_args.len(), oversize_args.as_ptr());

        let after_ns = str_value("core");
        let after_key = str_value("detached-after");
        let after_val = str_value("v2");
        let after_args = [Value::from_int(handle), after_ns, after_key, after_val];
        let after_pending = actor_send(pool, 0, after_args.len(), after_args.as_ptr());

        let before_result = result_unwrap(pending_await(before_pending));
        let oversize_result = result_unwrap(pending_await(oversize_pending));
        let after_result = result_unwrap(pending_await(after_pending));
        assert!(int_value(before_result).unwrap_or(0) > 0);
        assert!(int_value(oversize_result).unwrap_or(0) > 0);
        assert!(int_value(after_result).unwrap_or(0) > 0);

        assert_eq!(
            read_point(handle, b"core".to_vec(), b"detached-before".to_vec()).expect("read before"),
            Some(b"v1".to_vec())
        );
        assert_eq!(
            read_point(handle, b"core".to_vec(), b"detached-after".to_vec()).expect("read after"),
            Some(b"v2".to_vec())
        );

        let groups = writer_groups_for_handle(handle);
        let mid = groups
            .iter()
            .position(|group| group.kinds == vec![WriteEnvelopeKind::ClientBatch])
            .expect("detached oversize group");
        assert!(mid > 0 && mid + 1 < groups.len());

        unsafe {
            wr_rc_dec(before_ns);
            wr_rc_dec(before_key);
            wr_rc_dec(before_val);
            wr_rc_dec(after_ns);
            wr_rc_dec(after_key);
            wr_rc_dec(after_val);
            wr_rc_dec(before_pending);
            wr_rc_dec(oversize_pending);
            wr_rc_dec(after_pending);
            wr_rc_dec(before_result);
            wr_rc_dec(oversize_result);
            wr_rc_dec(after_result);
            wr_rc_dec(pool);
            wr_rc_dec(handles);
            wr_rc_dec(actor);
        }
        assert!(close_db(handle));
    }

    #[test]
    fn mixed_shard_batch_is_rejected_with_typed_error() {
        let dir = temp_dir();
        let handle = open_with_test_options(&dir, "ord", 64, None, vec![]).expect("open db");
        let shard_a = shard_for_key(handle, b"core".to_vec(), b"key-a".to_vec()).expect("route a");
        let mut key_b = b"key-b".to_vec();
        for idx in 0..2048 {
            let candidate = format!("key-b-{idx}").into_bytes();
            let shard =
                shard_for_key(handle, b"core".to_vec(), candidate.clone()).expect("route b");
            if shard != shard_a {
                key_b = candidate;
                break;
            }
        }
        let shard_b =
            shard_for_key(handle, b"core".to_vec(), key_b.clone()).expect("route b final");
        assert_ne!(shard_a, shard_b);
        let err = submit_batch(
            handle,
            &[
                BatchOp::Put {
                    namespace: Bytes::from_static(b"core"),
                    key: Bytes::from_static(b"key-a"),
                    value: Bytes::from_static(b"1"),
                    expected_version: None,
                },
                BatchOp::Put {
                    namespace: Bytes::from_static(b"core"),
                    key: key_b.into(),
                    value: Bytes::from_static(b"2"),
                    expected_version: None,
                },
            ],
        )
        .expect_err("mixed shard batch should fail");
        assert_eq!(err.code, ErrorCode::MixedShardBatchUnsupported);
        assert!(close_db(handle));
    }

    #[test]
    fn cross_shard_txn_lock_is_rejected_with_typed_error() {
        let dir = temp_dir();
        let handle = open_with_test_options(&dir, "ord", 64, None, vec![]).expect("open db");
        let txn_id = txn_begin(handle).expect("begin txn");
        txn_lock_key(handle, txn_id, b"core".to_vec(), b"key-a".to_vec()).expect("first lock");
        let shard_a = shard_for_key(handle, b"core".to_vec(), b"key-a".to_vec()).expect("route a");
        let mut key_b = b"key-b".to_vec();
        for idx in 0..2048 {
            let candidate = format!("key-b-{idx}").into_bytes();
            let shard =
                shard_for_key(handle, b"core".to_vec(), candidate.clone()).expect("route b");
            if shard != shard_a {
                key_b = candidate;
                break;
            }
        }
        let err = txn_lock_key(handle, txn_id, b"core".to_vec(), key_b)
            .expect_err("cross shard txn should fail");
        assert_eq!(err.code, ErrorCode::CrossShardTxnUnsupported);
        assert!(close_db(handle));
    }

    #[test]
    fn sovereignty_write_denies_disallowed_region() {
        let dir = temp_dir();
        let policy = ResidencyPolicy::with_rules(vec![ResidencyRule {
            shard: b"core".to_vec(),
            allowed_regions: vec!["ord".to_string()],
        }]);
        let handle =
            open_with_test_options(&dir, "iad", 1, Some(policy), vec!["iad"]).expect("open db");
        let err = submit_put(handle, b"core".to_vec(), b"k".to_vec(), b"v".to_vec(), None)
            .expect_err("write should be denied");
        assert_eq!(err.code, ErrorCode::SovereigntyWriteDenied);
        assert!(close_db(handle));
    }

    #[test]
    fn sovereignty_read_mode_is_policy_capped() {
        let dir = temp_dir();
        let writer_policy = ResidencyPolicy::with_rules(vec![ResidencyRule {
            shard: b"core".to_vec(),
            allowed_regions: vec!["ord".to_string()],
        }]);
        let writer = open_with_test_options(&dir, "ord", 1, Some(writer_policy), vec!["ord"])
            .expect("open writer");
        submit_put(writer, b"core".to_vec(), b"k".to_vec(), b"v".to_vec(), None)
            .expect("seed write");
        assert!(close_db(writer));

        let reader_policy = ResidencyPolicy::with_rules_and_options(
            vec![ResidencyRule {
                shard: b"core".to_vec(),
                allowed_regions: vec!["ord".to_string()],
            }],
            ReadSovereigntyCap::PolicyCappedClientChoice,
            vec!["ord".to_string()],
        );
        let reader = open_with_test_options(&dir, "iad", 1, Some(reader_policy), vec!["ord"])
            .expect("open reader");

        let strict_err = read_point_consistent_with_sovereignty(
            reader,
            b"core".to_vec(),
            b"k".to_vec(),
            ReadConsistency::Strong,
            None,
            ReadSovereigntyMode::Strict,
        )
        .expect_err("strict mode should deny out-of-region read");
        assert_eq!(strict_err.code, ErrorCode::SovereigntyReadDenied);

        let stale = read_point_consistent_with_sovereignty(
            reader,
            b"core".to_vec(),
            b"k".to_vec(),
            ReadConsistency::Eventual,
            None,
            ReadSovereigntyMode::StaleOk,
        )
        .expect("stale read allowed by policy cap");
        assert_eq!(stale, Some(b"v".to_vec()));
        assert!(close_db(reader));
    }

    #[test]
    fn async_failover_promotion_succeeds_for_allowed_region() {
        let dir = temp_dir();
        let mut checkpoint = crate::db::checkpoint::CheckpointConfig::default();
        checkpoint.allowed_regions = vec!["ord".to_string(), "iad".to_string()];
        let config = DbConfig::for_testing()
            .with_checkpoint(checkpoint)
            .with_replication(config::ReplicationConfig {
                async_failover: true,
                ..DbConfig::for_testing().replication
            })
            .with_topology(config::TopologyConfig {
                local_region: "ord".to_string(),
                region_az_node_map: std::collections::BTreeMap::from([
                    (
                        "ord".to_string(),
                        std::collections::BTreeMap::from([(
                            "ord".to_string(),
                            vec!["ord-1".to_string()],
                        )]),
                    ),
                    (
                        "iad".to_string(),
                        std::collections::BTreeMap::from([(
                            "iad".to_string(),
                            vec!["iad-1".to_string()],
                        )]),
                    ),
                ]),
                ..DbConfig::for_testing().topology
            });
        let config = DbConfig {
            sovereignty: config::SovereigntyConfig {
                id: "us".to_string(),
                allowed_regions: vec!["iad".to_string(), "ord".to_string()],
                enforce_all_copies: true,
            },
            ..config
        };
        let handle = open_db_with_config(&dir, &config).expect("open db");
        let owner = resolve_owner(handle, b"core".to_vec(), b"async-failover".to_vec())
            .expect("resolve owner");
        assert_eq!(owner.home_region, "ord");
        let promoted = promote_async_failover(
            handle,
            owner.keyrange_id.clone(),
            "iad".to_string(),
            owner.home_epoch,
        )
        .expect("promote async failover");
        assert_eq!(promoted.home_region, "iad");
        assert!(promoted.home_epoch > owner.home_epoch);
        assert!(close_db(handle));
    }

    #[test]
    fn async_failover_rejected_promotion_does_not_mutate_home_state() {
        let dir = temp_dir();
        let mut checkpoint = crate::db::checkpoint::CheckpointConfig::default();
        checkpoint.allowed_regions = vec!["ord".to_string(), "iad".to_string()];
        let config = DbConfig::for_testing()
            .with_checkpoint(checkpoint)
            .with_replication(config::ReplicationConfig {
                async_failover: true,
                ..DbConfig::for_testing().replication
            })
            .with_topology(config::TopologyConfig {
                local_region: "ord".to_string(),
                region_az_node_map: std::collections::BTreeMap::from([
                    (
                        "ord".to_string(),
                        std::collections::BTreeMap::from([(
                            "ord".to_string(),
                            vec!["ord-1".to_string()],
                        )]),
                    ),
                    (
                        "iad".to_string(),
                        std::collections::BTreeMap::from([(
                            "iad".to_string(),
                            vec!["iad-1".to_string()],
                        )]),
                    ),
                ]),
                ..DbConfig::for_testing().topology
            });
        let config = DbConfig {
            sovereignty: config::SovereigntyConfig {
                id: "us".to_string(),
                allowed_regions: vec!["iad".to_string(), "ord".to_string()],
                enforce_all_copies: true,
            },
            ..config
        };
        let handle = open_db_with_config(&dir, &config).expect("open db");
        let owner = resolve_owner(handle, b"core".to_vec(), b"async-failover-reject".to_vec())
            .expect("resolve owner");
        assert_eq!(owner.home_region, "ord");

        let err = promote_async_failover(
            handle,
            owner.keyrange_id.clone(),
            "iad".to_string(),
            owner.home_epoch.saturating_add(1),
        )
        .expect_err("stale expected epoch must fail");
        assert!(
            err.message.contains("ASYNC_PROMOTION_EPOCH_MISMATCH"),
            "unexpected error: {}",
            err.message
        );

        let after = resolve_owner(handle, b"core".to_vec(), b"async-failover-reject".to_vec())
            .expect("resolve owner after failure");
        assert_eq!(
            after.home_region, owner.home_region,
            "failed promotion must not mutate home region"
        );
        assert_eq!(
            after.home_epoch, owner.home_epoch,
            "failed promotion must not mutate home epoch"
        );
        assert_eq!(
            after.ownership_token, owner.ownership_token,
            "failed promotion must not mutate ownership token"
        );
        assert!(close_db(handle));
    }

    #[test]
    fn topology_open_fails_closed_when_canonical_topology_map_is_empty() {
        let dir = temp_dir();
        let config = DbConfig::for_testing().with_topology(config::TopologyConfig {
            local_region: " ORd ".to_string(),
            region_az_node_map: BTreeMap::new(),
            ..DbConfig::for_testing().topology
        });
        let err = open_db_with_config(&dir, &config).expect_err("empty topology must fail");
        assert!(
            err.message
                .contains("topology.local_region missing from canonical topology map"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn restore_on_open_fails_closed_when_restore_errors() {
        let dir = temp_dir();
        let checkpoint_dir = dir.join(".checkpoints");
        std::fs::create_dir_all(&checkpoint_dir).expect("create checkpoint dir");
        std::fs::write(checkpoint_dir.join("LATEST"), b"missing-checkpoint-id")
            .expect("write latest pointer");

        let wal_path = wal_path_from(&dir);
        let mut config = DbConfig::for_testing();
        config.restore_latest_checkpoint_on_open = true;

        let err = DbEngine::open_with_config(&wal_path, &config)
            .expect_err("restore failure must fail closed");
        assert_eq!(err.code, ErrorCode::Io);
        assert!(
            err.message.contains("CHECKPOINT_RESTORE_ON_OPEN_FAILED"),
            "unexpected error: {}",
            err.message
        );
        assert!(
            err.message.contains("no recoverable checkpoint found"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn open_db_with_config_rejects_non_majority_quorum() {
        let dir = temp_dir();
        let config = DbConfig::for_testing()
            .with_replication(config::ReplicationConfig {
                factor: 4,
                write_quorum: 2,
                ..DbConfig::for_testing().replication
            })
            .with_topology(config::TopologyConfig {
                local_region: "ord".to_string(),
                region_az_node_map: collapsed_topology_map_for("ord"),
                ..DbConfig::for_testing().topology
            });

        let err = open_db_with_config(&dir, &config).expect_err("non-majority quorum must fail");
        assert_eq!(err.code, ErrorCode::InvalidArgument);
        assert!(
            err.message
                .contains("replication.write_quorum must be majority quorum"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn db_engine_open_with_config_rejects_invalid_replication_inputs_without_normalizing() {
        let dir = temp_dir();
        let wal_path = wal_path_from(&dir);
        let base = DbConfig::for_testing().with_topology(config::TopologyConfig {
            local_region: "ord".to_string(),
            region_az_node_map: collapsed_topology_map_for("ord"),
            ..DbConfig::for_testing().topology
        });

        let zero_factor = base.clone().with_replication(config::ReplicationConfig {
            factor: 0,
            write_quorum: 0,
            ..DbConfig::for_testing().replication
        });
        let err =
            DbEngine::open_with_config(&wal_path, &zero_factor).expect_err("factor=0 must fail");
        assert_eq!(err.code, ErrorCode::InvalidArgument);
        assert!(
            err.message.contains("replication.factor must be > 0"),
            "unexpected error: {}",
            err.message
        );

        let overflow_quorum = base.with_replication(config::ReplicationConfig {
            factor: 3,
            write_quorum: 99,
            ..DbConfig::for_testing().replication
        });
        let err = DbEngine::open_with_config(&wal_path, &overflow_quorum)
            .expect_err("write_quorum overflow must fail");
        assert_eq!(err.code, ErrorCode::InvalidArgument);
        assert!(
            err.message
                .contains("replication.write_quorum must be within [1, 3]"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn restart_open_fails_closed_for_persisted_non_majority_quorum() {
        let dir = temp_dir();
        let config = DbConfig::for_testing()
            .with_replication(config::ReplicationConfig {
                factor: 3,
                write_quorum: 2,
                log_backend: ReplicatedLogBackend::DualWal,
                ..DbConfig::for_testing().replication
            })
            .with_topology(config::TopologyConfig {
                local_region: "ord".to_string(),
                region_az_node_map: collapsed_topology_map_for("ord"),
                ..DbConfig::for_testing().topology
            });
        let handle = open_db_with_config(&dir, &config).expect("open db");
        assert!(close_db(handle), "close must flush topology state");

        let wal_path = wal_path_from(&dir);
        let mut persisted =
            crate::db::topology::persistence::load_persisted_topology_state(&wal_path)
                .expect("load persisted topology")
                .expect("persisted topology");
        persisted.replication_factor = 3;
        persisted.write_quorum = 1;
        crate::db::topology::persistence::persist_topology_state(&wal_path, &persisted)
            .expect("persist invalid topology for restart test");

        let err = open_db(&dir).expect_err("restart must fail on persisted non-majority quorum");
        assert_eq!(err.code, ErrorCode::InvalidArgument);
        assert!(
            err.message
                .contains("persisted topology replication.write_quorum must be majority quorum"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn open_db_with_config_rejects_contradictory_intent() {
        let dir = temp_dir();
        let mut config = DbConfig::for_testing().with_topology(config::TopologyConfig {
            initial_logical_shards: 1,
            initial_active_groups: 1,
            local_region: "ord".to_string(),
            region_az_node_map: BTreeMap::from([(
                "ord".to_string(),
                BTreeMap::from([("az1".to_string(), vec!["ord-1".to_string()])]),
            )]),
            ..DbConfig::for_testing().topology
        });
        config.intent.policy_id = "contradictory-intent".to_string();
        config.intent.min_write_throughput_ops = 50_000;

        let err = open_db_with_config(&dir, &config).expect_err("contradictory intent must fail");
        assert_eq!(err.code, ErrorCode::InvalidArgument);
        assert!(
            err.message.contains("STRICT_CONFIG_INVALID"),
            "unexpected error: {}",
            err.message
        );
        assert!(
            err.message.contains("remediation:"),
            "unexpected error: {}",
            err.message
        );
        assert!(
            err.message.contains("increase available_nodes"),
            "unexpected error: {}",
            err.message
        );
        assert!(
            err.message.contains("increase logical_shards"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn topology_collapsed_mode_canonicalizes_region_sets_for_failover() {
        let dir = temp_dir();
        let mut checkpoint = crate::db::checkpoint::CheckpointConfig::default();
        checkpoint.allowed_regions = vec!["ord".to_string(), "iad".to_string(), "dfw".to_string()];
        let config = DbConfig::for_testing()
            .with_checkpoint(checkpoint)
            .with_replication(config::ReplicationConfig {
                async_failover: true,
                ..DbConfig::for_testing().replication
            })
            .with_topology(config::TopologyConfig {
                local_region: " ORd ".to_string(),
                region_az_node_map: BTreeMap::from([
                    (
                        "ORD".to_string(),
                        BTreeMap::from([("az-west-2".to_string(), vec!["ord-1".to_string()])]),
                    ),
                    (
                        "IAD".to_string(),
                        BTreeMap::from([("az-east-1".to_string(), vec!["iad-1".to_string()])]),
                    ),
                ]),
                ..DbConfig::for_testing().topology
            });
        let config = DbConfig {
            sovereignty: config::SovereigntyConfig {
                id: "us".to_string(),
                allowed_regions: vec!["ORD".to_string(), "iad".to_string()],
                enforce_all_copies: true,
            },
            ..config
        };
        let handle = open_db_with_config(&dir, &config).expect("open db");
        let owner = resolve_owner(handle, b"core".to_vec(), b"collapse-mode".to_vec())
            .expect("resolve owner");
        assert_eq!(owner.home_region, "ord");
        assert_eq!(owner.async_failover_regions, vec!["iad".to_string()]);

        let reopened = with_engine(handle, |engine| Ok(engine.reopen_config_for_restore()))
            .expect("reopen config");
        assert_eq!(reopened.intent, config.intent);
        assert!(
            reopened
                .topology
                .region_az_node_map
                .get("ord")
                .is_some_and(|az_map| az_map.contains_key("az-west-2"))
        );
        assert!(
            reopened
                .topology
                .region_az_node_map
                .get("iad")
                .is_some_and(|az_map| az_map.contains_key("az-east-1"))
        );
        let promoted = promote_async_failover(
            handle,
            owner.keyrange_id.clone(),
            " IAD ".to_string(),
            owner.home_epoch,
        )
        .expect("promote async failover");
        assert_eq!(promoted.home_region, "iad");
        assert!(close_db(handle));
    }

    #[test]
    fn checkpoint_region_policy_denies_disallowed_region() {
        let dir = temp_dir();
        let handle = open_with_test_options(&dir, "iad", 1, None, vec!["ord"]).expect("open db");
        let err = checkpoint_create(handle).expect_err("checkpoint create should be denied");
        assert_eq!(err.code, ErrorCode::SovereigntyCheckpointRegionDenied);
        assert!(close_db(handle));
    }

    #[test]
    fn writer_groups_never_mix_logical_shards() {
        let dir = temp_dir();
        let handle = open_with_test_options(&dir, "ord", 64, None, vec![]).expect("open db");
        submit_put(
            handle,
            b"core".to_vec(),
            b"key-a".to_vec(),
            b"1".to_vec(),
            None,
        )
        .expect("put a");
        let shard_a = shard_for_key(handle, b"core".to_vec(), b"key-a".to_vec()).expect("route a");
        let mut key_b = b"key-b".to_vec();
        for idx in 0..2048 {
            let candidate = format!("key-b-{idx}").into_bytes();
            let shard =
                shard_for_key(handle, b"core".to_vec(), candidate.clone()).expect("route b");
            if shard != shard_a {
                key_b = candidate;
                break;
            }
        }
        submit_put(handle, b"core".to_vec(), key_b, b"2".to_vec(), None).expect("put b");

        let groups = writer_groups_for_handle(handle);
        for group in groups {
            let first = *group.logical_shards.first().expect("group shard");
            assert!(
                group.logical_shards.iter().all(|shard| *shard == first),
                "writer group mixed shards: {:?}",
                group.logical_shards
            );
        }
        assert!(close_db(handle));
    }

    #[test]
    fn health_status_reports_writer_lane_metrics() {
        let dir = temp_dir();
        let config = DbConfig::for_testing()
            .with_replication(config::ReplicationConfig {
                log_backend: ReplicatedLogBackend::DualWal,
                ..DbConfig::for_testing().replication
            })
            .with_topology(config::TopologyConfig {
                initial_logical_shards: 8,
                local_region: "ord".to_string(),
                region_az_node_map: collapsed_topology_map_for("ord"),
                ..Default::default()
            })
            .with_engine(config::EngineConfig {
                writer_lane_count: 2,
                ..DbConfig::for_testing().engine
            });
        let handle = open_db_with_config(&dir, &config).expect("open db");

        let mut shard_seed_keys: HashMap<u32, Vec<u8>> = HashMap::new();
        for idx in 0..4096u32 {
            let key = format!("lane-key-{idx:04}").into_bytes();
            let shard =
                shard_for_key(handle, b"core".to_vec(), key.clone()).expect("route lane key");
            shard_seed_keys.entry(shard).or_insert(key);
            if shard_seed_keys.len() >= 4 {
                break;
            }
        }
        assert!(
            shard_seed_keys.len() >= 2,
            "test setup must find keys spanning at least two logical shards"
        );

        for repeat in 0..16u32 {
            for seed in shard_seed_keys.values() {
                let mut key = seed.clone();
                key.extend_from_slice(format!("-r{repeat:02}").as_bytes());
                submit_put(handle, b"core".to_vec(), key, b"v".to_vec(), None).expect("put");
            }
        }

        let health = db_health_status(handle).expect("health");
        assert_eq!(health.writer_lanes.len(), 2);
        let total_attempts = health
            .writer_lanes
            .iter()
            .map(|lane| lane.enqueue_attempts)
            .sum::<u64>();
        assert!(
            total_attempts >= 64,
            "expected enqueue attempts to be tracked"
        );
        assert!(
            health.writer_lanes.iter().all(|lane| lane.lane_id < 2),
            "lane ids should stay within configured lane count"
        );
        let assigned_total = health
            .writer_lanes
            .iter()
            .map(|lane| lane.assigned_shards)
            .sum::<u64>();
        assert!(
            assigned_total > 0,
            "writer lanes should report non-zero assigned shard counts"
        );
        assert!(
            health
                .writer_lanes
                .iter()
                .filter(|lane| lane.assigned_shards > 0)
                .count()
                >= 2,
            "multi-lane config should assign shards across lanes"
        );
        let max_assigned = health
            .writer_lanes
            .iter()
            .map(|lane| lane.assigned_shards)
            .max()
            .unwrap_or(0);
        assert!(
            max_assigned.saturating_mul(100) < assigned_total.saturating_mul(100),
            "multi-lane config should avoid 100% shard ownership by one lane"
        );
        assert!(
            health.writer_lane_max_enqueue_share_bps > 0,
            "writer lane enqueue share bps should be populated"
        );
        assert!(
            health.writer_lane_max_retry_after_bps <= 10_000,
            "retry-after share bps should stay bounded"
        );
        assert!(
            health.writer_lane_max_saturation_bps <= 10_000,
            "saturation share bps should stay bounded"
        );
        assert!(
            health.writer_lane_assignment_lookups > 0,
            "writer lane assignment lookups should be tracked"
        );
        assert!(
            health.writer_lane_assignment_hits > 0,
            "writer lane assignment hits should be tracked"
        );
        assert!(
            health.writer_lane_assignment_misses > 0,
            "writer lane assignment misses should be tracked"
        );
        assert_eq!(
            health.writer_lane_assignment_hits + health.writer_lane_assignment_misses,
            health.writer_lane_assignment_lookups,
            "writer lane assignment hit/miss counters should account for all lookups"
        );
        assert!(
            health.writer_lane_assignment_hit_rate_bps <= 10_000,
            "writer lane assignment hit-rate bps should stay bounded"
        );

        assert!(close_db(handle));
    }

    #[test]
    fn active_groups_maintain_independent_raft_log_progress() {
        let dir = temp_dir();
        let handle = open_with_test_options(&dir, "ord", 64, None, vec![]).expect("open db");

        let (key_a, group_a, key_b, group_b) = {
            let db = db_for_handle(handle).expect("db handle");
            let engine = db.write().expect("DB engine lock");
            let mut first: Option<(Vec<u8>, u32)> = None;
            let mut second: Option<(Vec<u8>, u32)> = None;
            for idx in 0..8192 {
                let key = format!("group-key-{idx}").into_bytes();
                let route = engine.route_key_to_shard(b"core", &key).expect("route");
                if let Some((_, existing_group)) = first.as_ref() {
                    if *existing_group != route.active_group_id {
                        second = Some((key, route.active_group_id));
                        break;
                    }
                } else {
                    first = Some((key, route.active_group_id));
                }
            }
            let (first_key, first_group) = first.expect("first group key");
            let (second_key, second_group) = second.expect("second group key");
            (first_key, first_group, second_key, second_group)
        };

        submit_put(
            handle,
            b"core".to_vec(),
            key_a.clone(),
            b"v-a".to_vec(),
            None,
        )
        .expect("put key a");

        let (after_first_group_a, after_first_group_b) = {
            let db = db_for_handle(handle).expect("db handle");
            let engine = db.write().expect("DB engine lock");
            (
                engine
                    .replication_groups
                    .get(&group_a)
                    .expect("group a")
                    .leader
                    .last_log_index(),
                engine
                    .replication_groups
                    .get(&group_b)
                    .expect("group b")
                    .leader
                    .last_log_index(),
            )
        };

        submit_put(handle, b"core".to_vec(), key_b, b"v-b".to_vec(), None).expect("put key b");

        let (after_second_group_a, after_second_group_b) = {
            let db = db_for_handle(handle).expect("db handle");
            let engine = db.write().expect("DB engine lock");
            (
                engine
                    .replication_groups
                    .get(&group_a)
                    .expect("group a")
                    .leader
                    .last_log_index(),
                engine
                    .replication_groups
                    .get(&group_b)
                    .expect("group b")
                    .leader
                    .last_log_index(),
            )
        };

        assert!(
            after_second_group_a >= after_first_group_a,
            "group A log index regressed"
        );
        assert!(
            after_second_group_b > after_first_group_b,
            "group B did not advance on group-B write"
        );
        assert_eq!(
            after_second_group_a, after_first_group_a,
            "group-A log should remain unchanged when writing group-B key"
        );

        assert!(close_db(handle));
    }

    #[test]
    fn cdc_emits_committed_apply_order_with_stable_commit_sequence() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        submit_batch(
            handle,
            &[
                BatchOp::Put {
                    namespace: Bytes::from_static(b"core"),
                    key: Bytes::from_static(b"k1"),
                    value: Bytes::from_static(b"v1"),
                    expected_version: None,
                },
                BatchOp::Delete {
                    namespace: Bytes::from_static(b"core"),
                    key: Bytes::from_static(b"k1"),
                    expected_version: None,
                },
            ],
        )
        .expect("submit batch");

        let db = db_for_handle(handle).expect("db handle");
        let engine = db.write().expect("DB engine lock");
        let events = engine.cdc_events(0, 16);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].commit_seq, 1);
        assert_eq!(events[0].kind, CdcOpKind::Put);
        assert_eq!(events[0].shard, b"core".to_vec());
        assert_eq!(events[1].commit_seq, 2);
        assert_eq!(events[1].kind, CdcOpKind::Delete);
        assert_eq!(events[1].shard, b"core".to_vec());
        drop(engine);
        assert!(close_db(handle));
    }

    #[test]
    fn cdc_never_emits_for_aborted_transactions() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let txn_id = txn_begin(handle).expect("begin txn");
        txn_abort(handle, txn_id).expect("abort txn");

        let db = db_for_handle(handle).expect("db handle");
        let engine = db.write().expect("DB engine lock");
        let events = engine.cdc_events(0, 16);
        assert!(events.is_empty());
        drop(engine);
        assert!(close_db(handle));
    }

    #[test]
    fn cdc_page_paginates_with_resume_cursor() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        submit_batch(
            handle,
            &[
                BatchOp::Put {
                    namespace: Bytes::from_static(b"core"),
                    key: Bytes::from_static(b"k1"),
                    value: Bytes::from_static(b"v1"),
                    expected_version: None,
                },
                BatchOp::Put {
                    namespace: Bytes::from_static(b"core"),
                    key: Bytes::from_static(b"k2"),
                    value: Bytes::from_static(b"v2"),
                    expected_version: None,
                },
                BatchOp::Delete {
                    namespace: Bytes::from_static(b"core"),
                    key: Bytes::from_static(b"k1"),
                    expected_version: None,
                },
            ],
        )
        .expect("submit batch");

        let first = cdc_page(handle, 0, 2, None).expect("first page");
        assert_eq!(first.events.len(), 2);
        assert_eq!(first.events[0].commit_seq, 1);
        assert_eq!(first.events[1].commit_seq, 2);
        assert_eq!(first.next_commit_seq, 2);
        assert!(first.high_watermark >= first.next_commit_seq);

        let second = cdc_page(handle, first.next_commit_seq, 2, None).expect("second page");
        assert_eq!(second.events.len(), 1);
        assert_eq!(second.events[0].commit_seq, 3);
        assert_eq!(second.next_commit_seq, 3);
        assert!(close_db(handle));
    }

    #[test]
    fn cdc_page_honors_shard_filter_and_keeps_monotonic_cursor() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        submit_batch(
            handle,
            &[
                BatchOp::Put {
                    namespace: Bytes::from_static(b"core"),
                    key: Bytes::from_static(b"k1"),
                    value: Bytes::from_static(b"v1"),
                    expected_version: None,
                },
                BatchOp::Put {
                    namespace: Bytes::from_static(b"aux"),
                    key: Bytes::from_static(b"k9"),
                    value: Bytes::from_static(b"v9"),
                    expected_version: None,
                },
            ],
        )
        .expect("submit batch");

        let core_only = cdc_page(handle, 0, 16, Some(b"core".to_vec())).expect("core page");
        assert_eq!(core_only.events.len(), 1);
        assert_eq!(core_only.events[0].shard, b"core".to_vec());
        assert_eq!(core_only.next_commit_seq, core_only.events[0].commit_seq);
        assert!(core_only.high_watermark >= 2);

        let missing = cdc_page(handle, 0, 16, Some(b"missing".to_vec())).expect("missing page");
        assert!(missing.events.is_empty());
        assert_eq!(missing.next_commit_seq, 0);
        assert!(missing.high_watermark >= 2);
        assert!(close_db(handle));
    }

    #[test]
    fn cdc_ack_checkpoint_is_monotonic_and_stream_scoped() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        assert_eq!(cdc_ack(handle, "orders".to_string(), 10).expect("ack"), 10);
        assert_eq!(
            cdc_ack(handle, "orders".to_string(), 7).expect("ack stale"),
            10
        );
        assert_eq!(
            cdc_ack(handle, "inventory".to_string(), 4).expect("ack other"),
            4
        );
        assert_eq!(
            cdc_checkpoint(handle, "orders".to_string()).expect("checkpoint"),
            Some(10)
        );
        assert_eq!(
            cdc_checkpoint(handle, "inventory".to_string()).expect("checkpoint"),
            Some(4)
        );
        assert_eq!(
            cdc_checkpoint(handle, "missing".to_string()).expect("checkpoint"),
            None
        );
        assert!(close_db(handle));
    }

    #[test]
    fn cdc_checkpoint_persists_across_restart() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        assert_eq!(cdc_ack(handle, "orders".to_string(), 12).expect("ack"), 12);
        assert!(close_db(handle));

        let reopened = open_db(&dir).expect("reopen db");
        assert_eq!(
            cdc_checkpoint(reopened, "orders".to_string()).expect("checkpoint"),
            Some(12)
        );
        assert!(close_db(reopened));
    }

    #[test]
    fn cdc_stream_page_resumes_from_stored_checkpoint() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        submit_batch(
            handle,
            &[
                BatchOp::Put {
                    namespace: Bytes::from_static(b"core"),
                    key: Bytes::from_static(b"k1"),
                    value: Bytes::from_static(b"v1"),
                    expected_version: None,
                },
                BatchOp::Put {
                    namespace: Bytes::from_static(b"core"),
                    key: Bytes::from_static(b"k2"),
                    value: Bytes::from_static(b"v2"),
                    expected_version: None,
                },
            ],
        )
        .expect("submit batch");
        assert_eq!(cdc_ack(handle, "orders".to_string(), 1).expect("ack"), 1);
        let page = cdc_stream_page(handle, "orders".to_string(), 8, None).expect("stream page");
        assert_eq!(page.events.len(), 1);
        assert_eq!(page.events[0].commit_seq, 2);
        assert!(close_db(handle));
    }

    #[test]
    fn cdc_stream_backfill_then_tail_uses_checkpoint_after_first_ack() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        submit_batch(
            handle,
            &[
                BatchOp::Put {
                    namespace: Bytes::from_static(b"core"),
                    key: Bytes::from_static(b"k1"),
                    value: Bytes::from_static(b"v1"),
                    expected_version: None,
                },
                BatchOp::Put {
                    namespace: Bytes::from_static(b"core"),
                    key: Bytes::from_static(b"k2"),
                    value: Bytes::from_static(b"v2"),
                    expected_version: None,
                },
                BatchOp::Put {
                    namespace: Bytes::from_static(b"core"),
                    key: Bytes::from_static(b"k3"),
                    value: Bytes::from_static(b"v3"),
                    expected_version: None,
                },
            ],
        )
        .expect("submit batch");

        // First call backfills from sequence 2 (inclusive).
        let first = cdc_stream_backfill_page(handle, "orders".to_string(), 2, 8, None)
            .expect("backfill page");
        assert_eq!(first.events.len(), 2);
        assert_eq!(first.events[0].commit_seq, 2);
        assert_eq!(first.events[1].commit_seq, 3);
        assert_eq!(cdc_ack(handle, "orders".to_string(), 2).expect("ack"), 2);

        // Subsequent call should tail from checkpoint=2, not restart from backfill start.
        let second =
            cdc_stream_backfill_page(handle, "orders".to_string(), 2, 8, None).expect("tail page");
        assert_eq!(second.events.len(), 1);
        assert_eq!(second.events[0].commit_seq, 3);
        assert!(close_db(handle));
    }

    #[test]
    fn cdc_duplicate_ack_storm_is_idempotent_across_restart() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        submit_batch(
            handle,
            &[BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"k1"),
                value: Bytes::from_static(b"v1"),
                expected_version: None,
            }],
        )
        .expect("submit batch");
        for _ in 0..20 {
            assert_eq!(cdc_ack(handle, "orders".to_string(), 1).expect("ack"), 1);
        }
        assert!(close_db(handle));

        let reopened = open_db(&dir).expect("reopen");
        for _ in 0..20 {
            assert_eq!(cdc_ack(reopened, "orders".to_string(), 1).expect("ack"), 1);
        }
        assert_eq!(
            cdc_checkpoint(reopened, "orders".to_string()).expect("checkpoint"),
            Some(1)
        );
        let page = cdc_stream_page(reopened, "orders".to_string(), 8, None).expect("stream");
        assert!(
            page.events.is_empty(),
            "checkpointed event must not replay unexpectedly"
        );
        assert!(close_db(reopened));
    }

    #[test]
    fn cdc_ack_persist_failure_does_not_advance_in_memory_checkpoint() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        submit_batch(
            handle,
            &[BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"k1"),
                value: Bytes::from_static(b"v1"),
                expected_version: None,
            }],
        )
        .expect("submit batch");
        assert_eq!(cdc_ack(handle, "orders".to_string(), 1).expect("ack"), 1);

        {
            let db = db_for_handle(handle).expect("db handle");
            let mut engine = db.write().expect("DB engine lock");
            engine.inject_cdc_checkpoint_persist_failure();
        }

        let err = cdc_ack(handle, "orders".to_string(), 2).expect_err("inject persist failure");
        assert_eq!(err.code, ErrorCode::Io);
        assert_eq!(
            cdc_checkpoint(handle, "orders".to_string()).expect("checkpoint"),
            Some(1),
            "checkpoint must remain unchanged when persist fails"
        );

        assert_eq!(
            cdc_ack(handle, "orders".to_string(), 2).expect("retry ack"),
            2
        );
        assert_eq!(
            cdc_checkpoint(handle, "orders".to_string()).expect("checkpoint"),
            Some(2)
        );
        assert!(close_db(handle));
    }

    #[test]
    fn submit_batch_enforces_quorum_gate_for_multi_voter_mode() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let db = db_for_handle(handle).expect("db handle");
        {
            let mut engine = db.write().expect("DB engine lock");
            engine.set_membership_voters([1, 2, 3]).expect("set voters");
            let stalled_term = engine
                .primary_replication()
                .leader
                .current_term
                .saturating_add(5);
            for follower in engine.primary_replication_mut().followers.values_mut() {
                follower.current_term = stalled_term;
            }
        }

        let err = submit_batch(
            handle,
            &[BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"k"),
                value: Bytes::from_static(b"v"),
                expected_version: None,
            }],
        )
        .expect_err("must fail without quorum responses");
        assert_eq!(err.code, ErrorCode::LimitExceeded);
        assert!(err.message.contains("durability quorum not reached"));
        assert!(
            err.message
                .contains(QUORUM_FAILURE_TOKEN_DURABILITY_NOT_REACHED)
        );
        let value = read_point(handle, b"core".to_vec(), b"k".to_vec()).expect("read after fail");
        assert!(value.is_none(), "quorum failure must not apply writes");
        assert!(close_db(handle));
    }

    #[test]
    fn submit_batch_applies_replication_in_flight_limit_backpressure() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let db = db_for_handle(handle).expect("db handle");
        {
            let mut engine = db.write().expect("DB engine lock");
            engine
                .set_membership_voters(1u64..=400u64)
                .expect("set voters");
        }

        let err = submit_batch(
            handle,
            &[BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"k-limit"),
                value: Bytes::from_static(b"v-limit"),
                expected_version: None,
            }],
        )
        .expect_err("replication target hard cap should apply backpressure");
        assert_eq!(err.code, ErrorCode::LimitExceeded);
        assert!(
            err.message
                .contains(QUORUM_FAILURE_TOKEN_REPLICATION_IN_FLIGHT_LIMIT)
        );
        let health = db_health_status(handle).expect("health");
        assert_eq!(
            health.quorum_failure_token.as_deref(),
            Some(QUORUM_FAILURE_TOKEN_REPLICATION_IN_FLIGHT_LIMIT)
        );
        assert!(
            health.replication_failure_counters.iter().any(|counter| {
                counter.token == QUORUM_FAILURE_TOKEN_REPLICATION_IN_FLIGHT_LIMIT
                    && counter.count > 0
            }),
            "failure counter should record in-flight limit backpressure"
        );
        assert!(health.replica_acks.is_empty());
        assert!(close_db(handle));
    }

    #[test]
    fn submit_batch_streams_replication_targets_over_in_flight_cap() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let db = db_for_handle(handle).expect("db handle");
        {
            let mut engine = db.write().expect("DB engine lock");
            engine
                .set_membership_voters(1u64..=40u64)
                .expect("set voters");
        }

        submit_batch(
            handle,
            &[BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"k-stream"),
                value: Bytes::from_static(b"v-stream"),
                expected_version: None,
            }],
        )
        .expect("replication fanout should stream in waves instead of backpressure failing");

        let value = read_point(handle, b"core".to_vec(), b"k-stream".to_vec()).expect("read");
        assert_eq!(value, Some(b"v-stream".to_vec()));
        let health = db_health_status(handle).expect("health");
        assert_ne!(health.quorum_ack_count, 0, "quorum ack should be recorded");
        assert_ne!(
            health.replica_acks.len(),
            0,
            "replica ack telemetry should be populated"
        );
        assert_eq!(
            health.replication_target_count, 39,
            "target count should capture all remote voters"
        );
        assert!(health.replication_wave_count >= 1);
        assert!(
            health.replication_contacted_count <= health.replication_target_count,
            "contacted count should be bounded by target count"
        );
        assert!(
            health.replication_successful_count <= health.replication_contacted_count,
            "successful fanout count should be bounded by contacted count"
        );
        assert!(
            health.replication_failed_count <= health.replication_contacted_count,
            "failed fanout count should be bounded by contacted count"
        );
        assert!(
            health.replication_cancelled_count <= health.replication_target_count,
            "cancelled fanout count should be bounded by target count"
        );
        assert!(
            health.replication_wave_avg_targets <= health.replication_wave_max_targets,
            "average wave size should not exceed max wave size"
        );
        assert_eq!(
            health.replication_skipped_count,
            health
                .replication_target_count
                .saturating_sub(health.replication_contacted_count),
            "skipped count should reflect uncontacted targets after quorum short-circuit"
        );
        assert_eq!(
            health.replication_aborted_in_flight_count, 0,
            "current wave fanout does not abort in-flight RPCs"
        );
        assert!(
            health.replica_acks.len() < 39,
            "quorum-aware replication should avoid contacting every remote voter"
        );
        assert_ne!(
            health.quorum_failure_token.as_deref(),
            Some(QUORUM_FAILURE_TOKEN_REPLICATION_IN_FLIGHT_LIMIT),
            "in-flight cap should no longer reject moderate target sets"
        );
        assert!(close_db(handle));
    }

    #[test]
    fn health_status_reports_replication_rpc_backpressure_snapshot() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let health = db_health_status(handle).expect("health");
        assert!(
            health.replication_rpc_max_in_flight > 0,
            "replication rpc in-flight cap should be positive"
        );
        assert!(
            health.replication_rpc_available_permits <= health.replication_rpc_max_in_flight,
            "available permits should be bounded by configured cap"
        );
        assert!(
            health.replication_rpc_in_flight <= health.replication_rpc_max_in_flight,
            "in-flight permits should be bounded by configured cap"
        );
        assert!(close_db(handle));
    }

    #[test]
    fn map_private_rpc_error_tags_backpressure_failures() {
        let err = map_private_rpc_error(crate::db::rpc::errors::RpcError {
            code: crate::db::rpc::errors::RpcStatusCode::Unavailable,
            message: "REPLICATION_RPC_BACKPRESSURE: timed out waiting for in-flight permit"
                .to_string(),
            retry: Some(crate::db::rpc::errors::RetryHint { retry_after_ms: 25 }),
            leader: None,
        });
        assert!(
            err.message
                .contains(QUORUM_FAILURE_TOKEN_REPLICATION_RPC_BACKPRESSURE),
            "backpressure mapping should stamp explicit quorum failure token"
        );
    }

    #[test]
    fn submit_batch_require_private_rpc_rejects_simulated_quorum_transport() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let db = db_for_handle(handle).expect("db handle");
        {
            let mut engine = db.write().expect("DB engine lock");
            engine.set_membership_voters([1, 2, 3]).expect("set voters");
            engine.quorum_transport_mode = QuorumTransportMode::RequirePrivateRpc;
        }

        let err = submit_batch(
            handle,
            &[BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"k-rpc-required"),
                value: Bytes::from_static(b"v-rpc-required"),
                expected_version: None,
            }],
        )
        .expect_err("require-private-rpc mode must reject local simulation fallback");
        assert_eq!(err.code, ErrorCode::LimitExceeded);
        assert!(
            err.message
                .contains(QUORUM_FAILURE_TOKEN_PRIVATE_RPC_REQUIRED),
            "expected explicit failure token for missing private rpc path"
        );
        let health = db_health_status(handle).expect("health");
        assert_eq!(
            health.quorum_transport_mode,
            QuorumTransportMode::RequirePrivateRpc
        );
        assert_eq!(
            health.replication_simulation_commits, 0,
            "simulation path should not execute when require-private-rpc is enabled"
        );
        assert_eq!(
            health.quorum_failure_token.as_deref(),
            Some(QUORUM_FAILURE_TOKEN_PRIVATE_RPC_REQUIRED)
        );
        assert!(close_db(handle));
    }

    #[test]
    fn submit_batch_prefer_mode_uses_simulation_fallback_without_mesh() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let db = db_for_handle(handle).expect("db handle");
        {
            let mut engine = db.write().expect("DB engine lock");
            engine.set_membership_voters([1, 2, 3]).expect("set voters");
            engine.quorum_transport_mode = QuorumTransportMode::PreferPrivateRpc;
        }

        submit_batch(
            handle,
            &[BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"k-sim-fallback"),
                value: Bytes::from_static(b"v-sim-fallback"),
                expected_version: None,
            }],
        )
        .expect("prefer mode should fall back to simulation without mesh");

        let health = db_health_status(handle).expect("health");
        assert!(health.replication_simulation_commits >= 1);
        assert!(health.quorum_failure_token.is_none());
        assert!(close_db(handle));
    }

    #[test]
    fn replication_outside_lock_active_path_uses_simulation_fallback_without_mesh() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let db = db_for_handle(handle).expect("db handle");
        {
            let mut engine = db.write().expect("DB engine lock");
            engine.set_membership_voters([1, 2, 3]).expect("set voters");
            engine.quorum_transport_mode = QuorumTransportMode::PreferPrivateRpc;
        }
        submit_batch(
            handle,
            &[BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"k-outside-sim"),
                value: Bytes::from_static(b"v-outside-sim"),
                expected_version: None,
            }],
        )
        .expect("submit with outside-lock simulation fallback");
        let observed = read_point(handle, b"core".to_vec(), b"k-outside-sim".to_vec())
            .expect("read")
            .expect("value");
        assert_eq!(observed, b"v-outside-sim".to_vec());
        let health = db_health_status(handle).expect("health");
        assert!(
            health.replication_simulation_commits >= 1,
            "outside-lock fallback should still account simulation commits"
        );
        assert!(close_db(handle));
    }

    #[test]
    fn shadow_replicated_log_backend_reports_overhead_telemetry() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let db = db_for_handle(handle).expect("db handle");
        {
            let mut engine = db.write().expect("DB engine lock");
            engine.replicated_log_backend = ReplicatedLogBackend::ShadowCanonical;
            engine
                .set_membership_voters([1])
                .expect("single voter mode");
            engine.write_quorum = 1;
        }

        submit_batch(
            handle,
            &[BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"k-shadow"),
                value: Bytes::from_static(b"v-shadow"),
                expected_version: None,
            }],
        )
        .expect("submit in shadow backend mode");

        let health = db_health_status(handle).expect("health");
        assert_eq!(
            health.replicated_log_backend,
            ReplicatedLogBackend::ShadowCanonical
        );
        assert!(
            health.replicated_log_shadow_wal_bytes >= health.replicated_log_shadow_payload_bytes,
            "wal bytes should be >= payload bytes in dual-log shadow accounting"
        );
        assert_eq!(
            health.replicated_log_shadow_overhead_bytes,
            health
                .replicated_log_shadow_wal_bytes
                .saturating_sub(health.replicated_log_shadow_payload_bytes)
        );
        assert!(close_db(handle));
    }

    #[test]
    fn additional_acks_needed_handles_simple_and_joint_membership() {
        let simple = MembershipConfig::new([1, 2, 3]).expect("simple membership");
        let only_leader = BTreeSet::from([1u64]);
        assert_eq!(
            additional_acks_needed_for_quorum(&simple, &only_leader, 2),
            1,
            "3-voter quorum should require one more durable ack"
        );
        assert_eq!(
            additional_acks_needed_for_quorum(&simple, &only_leader, 3),
            2,
            "write_quorum should raise minimum additional acknowledgements"
        );

        let mut joint = MembershipConfig::new([1, 2, 3]).expect("joint base");
        joint
            .begin_joint_change(MembershipChange::AddVoter { node_id: 4 }, 77)
            .expect("begin joint change");
        assert_eq!(
            additional_acks_needed_for_quorum(&joint, &only_leader, 2),
            2,
            "joint quorum should account for incoming voter set majority"
        );
        let old_quorum_acked = BTreeSet::from([1u64, 2u64]);
        assert_eq!(
            additional_acks_needed_for_quorum(&joint, &old_quorum_acked, 2),
            1,
            "joint quorum should shrink additional acks once outgoing quorum is already satisfied"
        );
    }

    #[test]
    fn map_remote_voters_to_mesh_nodes_is_stable_and_deduplicated() {
        let mapped = map_remote_voters_to_mesh_nodes(
            &[4, 2, 3, 3],
            vec![
                "node-c".to_string(),
                "node-a".to_string(),
                "node-b".to_string(),
                "node-b".to_string(),
            ],
        );
        assert_eq!(
            mapped,
            vec![
                (2, "node-a".to_string()),
                (3, "node-b".to_string()),
                (4, "node-c".to_string())
            ]
        );
    }

    #[test]
    fn submit_batch_accepts_quorum_with_append_responses() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let db = db_for_handle(handle).expect("db handle");
        {
            let mut engine = db.write().expect("DB engine lock");
            engine.set_membership_voters([1, 2, 3]).expect("set voters");
            engine.pending_append_responses = vec![FollowerAppendResponse {
                node_id: 2,
                response: AppendEntriesResponse {
                    term: 1,
                    success: true,
                    match_index: u64::MAX,
                    conflict_index: None,
                },
                replication_latency_ns: 10,
                fsync_latency_ns: 5,
            }];
        }

        submit_batch(
            handle,
            &[BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"k"),
                value: Bytes::from_static(b"v"),
                expected_version: None,
            }],
        )
        .expect("quorum satisfied");
        assert!(close_db(handle));
    }

    #[test]
    fn quorum_rejection_does_not_leak_operations_into_future_success() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let db = db_for_handle(handle).expect("db handle");
        {
            let mut engine = db.write().expect("DB engine lock");
            engine.set_membership_voters([1, 2, 3]).expect("set voters");
            let stalled_term = engine
                .primary_replication()
                .leader
                .current_term
                .saturating_add(5);
            for follower in engine.primary_replication_mut().followers.values_mut() {
                follower.current_term = stalled_term;
            }
        }

        let rejected = submit_batch(
            handle,
            &[BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"k1"),
                value: Bytes::from_static(b"v1"),
                expected_version: None,
            }],
        );
        assert!(
            rejected.is_err(),
            "first batch must be rejected without quorum"
        );

        {
            let mut engine = db.write().expect("DB engine lock");
            let current_term = engine.raft_current_term;
            for follower in engine.primary_replication_mut().followers.values_mut() {
                follower.current_term = current_term;
            }
        }

        submit_batch(
            handle,
            &[BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"k2"),
                value: Bytes::from_static(b"v2"),
                expected_version: None,
            }],
        )
        .expect("second batch should pass with quorum");

        let leaked = read_point(handle, b"core".to_vec(), b"k1".to_vec()).expect("read k1");
        let committed = read_point(handle, b"core".to_vec(), b"k2".to_vec()).expect("read k2");
        assert_eq!(
            leaked, None,
            "rejected op must never leak into later commit"
        );
        assert_eq!(committed, Some(b"v2".to_vec()));
        assert!(close_db(handle));
    }

    #[test]
    fn submit_batch_requires_term_and_index_fidelity_for_quorum() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let db = db_for_handle(handle).expect("db handle");
        {
            let mut engine = db.write().expect("DB engine lock");
            engine.set_membership_voters([1, 2, 3]).expect("set voters");
            engine.raft_current_term = 5;
            for follower in engine.primary_replication_mut().followers.values_mut() {
                follower.current_term = 4;
            }
        }
        let err = submit_batch(
            handle,
            &[BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"k"),
                value: Bytes::from_static(b"v"),
                expected_version: None,
            }],
        )
        .expect_err("stale-term response must not count toward quorum");
        assert_eq!(err.code, ErrorCode::LimitExceeded);
        let health = db_health_status(handle).expect("health");
        assert_eq!(
            health.quorum_failure_token.as_deref(),
            Some(QUORUM_FAILURE_TOKEN_DURABILITY_NOT_REACHED)
        );
        assert!(!health.replica_acks.is_empty());
        assert!(
            health.replica_acks.iter().all(|ack| !ack.durable_ack),
            "stale-term responses must never count as durable quorum acks"
        );
        assert!(close_db(handle));
    }

    #[test]
    fn joint_membership_requires_dual_quorum_in_live_write_path() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let db = db_for_handle(handle).expect("db handle");
        {
            let mut engine = db.write().expect("DB engine lock");
            engine.set_membership_voters([1, 2, 3]).expect("set voters");
            engine
                .begin_membership_change(MembershipChange::AddVoter { node_id: 4 }, 7)
                .expect("begin joint");
            let current_term = engine.raft_current_term;
            for follower in engine.primary_replication_mut().followers.values_mut() {
                follower.current_term = current_term.saturating_add(5);
            }
            if let Some(follower_two) = engine.primary_replication_mut().followers.get_mut(&2) {
                follower_two.current_term = current_term;
            }
        }

        let err = submit_batch(
            handle,
            &[BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"joint-k"),
                value: Bytes::from_static(b"v1"),
                expected_version: None,
            }],
        )
        .expect_err("single old-quorum ack must fail dual quorum");
        assert_eq!(err.code, ErrorCode::LimitExceeded);

        {
            let mut engine = db.write().expect("DB engine lock");
            let current_term = engine.raft_current_term;
            if let Some(follower_four) = engine.primary_replication_mut().followers.get_mut(&4) {
                follower_four.current_term = current_term;
            }
        }

        submit_batch(
            handle,
            &[BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"joint-k"),
                value: Bytes::from_static(b"v2"),
                expected_version: None,
            }],
        )
        .expect("dual quorum should pass");
        assert!(close_db(handle));
    }

    #[test]
    fn abort_membership_change_restores_quorum_immediately() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let db = db_for_handle(handle).expect("db handle");
        {
            let mut engine = db.write().expect("DB engine lock");
            engine.set_membership_voters([1, 2, 3]).expect("set voters");
            engine
                .begin_membership_change(MembershipChange::AddVoter { node_id: 4 }, 9)
                .expect("begin joint");
            engine.abort_membership_change().expect("abort joint");
            engine.pending_append_responses = vec![FollowerAppendResponse {
                node_id: 2,
                response: AppendEntriesResponse {
                    term: engine.raft_current_term,
                    success: true,
                    match_index: u64::MAX,
                    conflict_index: None,
                },
                replication_latency_ns: 10,
                fsync_latency_ns: 5,
            }];
        }

        submit_batch(
            handle,
            &[BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"abort-joint-k"),
                value: Bytes::from_static(b"v"),
                expected_version: None,
            }],
        )
        .expect("post-abort old quorum should apply");
        assert!(close_db(handle));
    }

    #[test]
    fn txn_lifecycle_enforces_transitions_and_hlc_monotonicity() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let txn_id = txn_begin(handle).expect("begin txn");
        txn_prepare(handle, txn_id).expect("prepare txn");
        txn_commit(handle, txn_id).expect("commit txn");

        let db = db_for_handle(handle).expect("db handle");
        let engine = db.write().expect("DB engine lock");
        let record = engine.txn_record(txn_id).expect("txn record");
        assert_eq!(record.state, TxnState::Committed);
        assert!(record.prepared_ts.expect("prepared ts") >= record.start_ts);
        assert!(record.commit_ts.expect("commit ts") >= record.prepared_ts.expect("prepared ts"));
        drop(engine);

        let txn2 = txn_begin(handle).expect("begin txn2");
        txn_abort(handle, txn2).expect("abort txn2");
        let commit_err = txn_commit(handle, txn2).expect_err("aborted txn cannot commit");
        assert_eq!(commit_err.code, ErrorCode::InvalidArgument);
        assert!(close_db(handle));
    }

    #[test]
    fn txn_key_lock_conflict_returns_retry_then_succeeds_after_abort() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let db = db_for_handle(handle).expect("db handle");

        let (txn1, txn2) = {
            let mut engine = db.write().expect("DB engine lock");
            let txn1 = engine.txn_begin().expect("txn1 begin");
            let txn2 = engine.txn_begin().expect("txn2 begin");
            engine
                .txn_lock_key(txn1, b"core", b"k1")
                .expect("txn1 lock key");
            let conflict = engine
                .txn_lock_key(txn2, b"core", b"k1")
                .expect_err("txn2 should conflict");
            assert_eq!(conflict.code, ErrorCode::LimitExceeded);
            assert!(conflict.message.contains("RETRY_AFTER_MS=25"));
            (txn1, txn2)
        };

        {
            let mut engine = db.write().expect("DB engine lock");
            engine.txn_abort(txn1).expect("abort txn1");
            engine
                .txn_lock_key(txn2, b"core", b"k1")
                .expect("txn2 lock after release");
        }
        assert!(close_db(handle));
    }

    #[test]
    fn txn_lock_deadlock_aborts_deterministic_victim() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let db = db_for_handle(handle).expect("db handle");

        let (txn1, txn2) = {
            let mut engine = db.write().expect("DB engine lock");
            let txn1 = engine.txn_begin().expect("txn1 begin");
            let txn2 = engine.txn_begin().expect("txn2 begin");

            engine
                .txn_lock_key(txn1, b"core", b"a")
                .expect("txn1 lock a");
            engine
                .txn_lock_key(txn2, b"core", b"b")
                .expect("txn2 lock b");

            let wait = engine
                .txn_lock_key(txn1, b"core", b"b")
                .expect_err("txn1 waits on txn2");
            assert!(wait.message.contains("RETRY_AFTER_MS=25"));

            let deadlock = engine
                .txn_lock_key(txn2, b"core", b"a")
                .expect_err("txn2 should be deadlock victim");
            assert!(
                deadlock
                    .message
                    .contains(&format!("deadlock victim txn={txn2}"))
            );
            (txn1, txn2)
        };

        {
            let mut engine = db.write().expect("DB engine lock");
            let victim = engine.txn_record(txn2).expect("txn2 record");
            assert_eq!(victim.state, TxnState::Aborted);
            engine
                .txn_lock_key(txn1, b"core", b"b")
                .expect("txn1 can acquire b after victim abort");
        }
        assert!(close_db(handle));
    }

    #[test]
    fn txn_lock_snapshot_exposes_held_locks_and_wait_edges() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let db = db_for_handle(handle).expect("db handle");

        {
            let mut engine = db.write().expect("DB engine lock");
            let txn1 = engine.txn_begin().expect("txn1 begin");
            let txn2 = engine.txn_begin().expect("txn2 begin");

            engine
                .txn_lock_key(txn1, b"core", b"a")
                .expect("txn1 lock a");
            engine
                .txn_lock_key(txn2, b"core", b"b")
                .expect("txn2 lock b");
            let _ = engine
                .txn_lock_key(txn1, b"core", b"b")
                .expect_err("txn1 waits on txn2");

            let snapshot = engine.lock_table_snapshot();
            assert_eq!(snapshot.held_locks.len(), 2);
            assert_eq!(snapshot.waits, vec![(txn1, txn2)]);
        }

        assert!(close_db(handle));
    }

    #[test]
    fn txn_lock_disjoint_keys_do_not_conflict_or_wait() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let db = db_for_handle(handle).expect("db handle");

        {
            let mut engine = db.write().expect("DB engine lock");
            let txn1 = engine.txn_begin().expect("txn1 begin");
            let txn2 = engine.txn_begin().expect("txn2 begin");

            engine
                .txn_lock_key(txn1, b"core", b"a")
                .expect("txn1 lock a");
            engine
                .txn_lock_key(txn2, b"core", b"b")
                .expect("txn2 lock b");

            let snapshot = engine.lock_table_snapshot();
            assert_eq!(snapshot.waits.len(), 0);
            assert_eq!(snapshot.held_locks.len(), 2);
        }

        assert!(close_db(handle));
    }

    #[test]
    fn txn_lock_range_conflicts_with_overlapping_key_and_disjoint_allows() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let db = db_for_handle(handle).expect("db handle");

        {
            let mut engine = db.write().expect("DB engine lock");
            let txn1 = engine.txn_begin().expect("txn1 begin");
            let txn2 = engine.txn_begin().expect("txn2 begin");
            let txn3 = engine.txn_begin().expect("txn3 begin");

            engine
                .txn_lock_range(txn1, b"core", b"a", b"m")
                .expect("txn1 range lock");

            let overlap = engine
                .txn_lock_key(txn2, b"core", b"k")
                .expect_err("overlapping key should conflict");
            assert_eq!(overlap.code, ErrorCode::LimitExceeded);
            assert!(overlap.message.contains("RETRY_AFTER_MS=25"));

            engine
                .txn_lock_key(txn3, b"core", b"z")
                .expect("disjoint key should acquire");
        }

        assert!(close_db(handle));
    }

    #[test]
    fn txn_lock_overlapping_ranges_conflict_and_invalid_range_is_rejected() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let db = db_for_handle(handle).expect("db handle");

        {
            let mut engine = db.write().expect("DB engine lock");
            let txn1 = engine.txn_begin().expect("txn1 begin");
            let txn2 = engine.txn_begin().expect("txn2 begin");
            let txn3 = engine.txn_begin().expect("txn3 begin");

            engine
                .txn_lock_range(txn1, b"core", b"a", b"m")
                .expect("txn1 range lock");

            let overlap = engine
                .txn_lock_range(txn2, b"core", b"h", b"z")
                .expect_err("overlapping range should conflict");
            assert_eq!(overlap.code, ErrorCode::LimitExceeded);
            assert!(overlap.message.contains("RETRY_AFTER_MS=25"));

            engine
                .txn_lock_range(txn3, b"core", b"m", b"z")
                .expect("touching boundary should not overlap");

            let invalid = engine
                .txn_lock_range(txn3, b"core", b"z", b"z")
                .expect_err("invalid range should fail");
            assert_eq!(invalid.code, ErrorCode::InvalidArgument);
        }

        assert!(close_db(handle));
    }

    #[test]
    fn txn_commit_releases_locks_for_waiting_transactions() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let db = db_for_handle(handle).expect("db handle");

        let (txn1, txn2) = {
            let mut engine = db.write().expect("DB engine lock");
            let txn1 = engine.txn_begin().expect("txn1 begin");
            let txn2 = engine.txn_begin().expect("txn2 begin");

            engine
                .txn_lock_key(txn1, b"core", b"k1")
                .expect("txn1 lock k1");
            let conflict = engine
                .txn_lock_key(txn2, b"core", b"k1")
                .expect_err("txn2 waits on txn1");
            assert_eq!(conflict.code, ErrorCode::LimitExceeded);
            assert!(conflict.message.contains("RETRY_AFTER_MS=25"));
            (txn1, txn2)
        };

        {
            let mut engine = db.write().expect("DB engine lock");
            engine.txn_commit(txn1).expect("commit txn1");
            engine
                .txn_lock_key(txn2, b"core", b"k1")
                .expect("txn2 acquires after commit release");
        }

        assert!(close_db(handle));
    }

    #[test]
    fn txn_lock_intents_do_not_survive_restart_recovery() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let db = db_for_handle(handle).expect("db handle");

        {
            let mut engine = db.write().expect("DB engine lock");
            let txn1 = engine.txn_begin().expect("txn1 begin");
            let txn2 = engine.txn_begin().expect("txn2 begin");
            engine
                .txn_lock_key(txn1, b"core", b"restart-k")
                .expect("txn1 lock");
            let conflict = engine
                .txn_lock_key(txn2, b"core", b"restart-k")
                .expect_err("txn2 should conflict before restart");
            assert!(conflict.message.contains("RETRY_AFTER_MS=25"));
        }

        assert!(close_db(handle));
        let reopened = open_db(&dir).expect("reopen db");
        let db = db_for_handle(reopened).expect("db handle");
        {
            let mut engine = db.write().expect("DB engine lock");
            let txn = engine.txn_begin().expect("txn begin");
            engine
                .txn_lock_key(txn, b"core", b"restart-k")
                .expect("restart should not retain stale intents");
        }
        assert!(close_db(reopened));
    }

    #[test]
    fn snapshot_restore_rewinds_state_in_place_and_keeps_handle_usable() {
        let dir = temp_dir();
        let handle = open_with_backend(&dir, ReplicatedLogBackend::DualWal).expect("open db");
        submit_put(
            handle,
            b"core".to_vec(),
            b"k-snapshot".to_vec(),
            b"v1".to_vec(),
            None,
        )
        .expect("put v1");
        let snapshot_id = snapshot_start(handle).expect("start snapshot");
        submit_put(
            handle,
            b"core".to_vec(),
            b"k-snapshot".to_vec(),
            b"v2".to_vec(),
            None,
        )
        .expect("put v2");
        assert_eq!(
            read_point(handle, b"core".to_vec(), b"k-snapshot".to_vec()).expect("read v2"),
            Some(b"v2".to_vec())
        );
        let progress = snapshot_status(handle, snapshot_id).expect("status");
        assert_eq!(progress, 100);
        restore_snapshot(handle, snapshot_id).expect("restore snapshot");
        assert_eq!(
            read_point(handle, b"core".to_vec(), b"k-snapshot".to_vec()).expect("read restored v1"),
            Some(b"v1".to_vec())
        );
        submit_put(
            handle,
            b"core".to_vec(),
            b"k-snapshot-after".to_vec(),
            b"v3".to_vec(),
            None,
        )
        .expect("post-restore write succeeds");
        assert_eq!(
            read_point(handle, b"core".to_vec(), b"k-snapshot-after".to_vec())
                .expect("post-restore read"),
            Some(b"v3".to_vec())
        );

        let missing = snapshot_status(handle, snapshot_id.saturating_add(99))
            .expect_err("unknown snapshot id");
        assert_eq!(missing.code, ErrorCode::InvalidArgument);
        assert!(close_db(handle));
    }

    #[test]
    fn snapshot_restore_reopen_config_preserves_intent() {
        let dir = temp_dir();
        let custom_intent = crate::db::autopilot::compiler::DbIntentConfig {
            policy_id: "snapshot-restore-intent".to_string(),
            latency_target_ms: 9,
            min_write_throughput_ops: 1_234,
            residency_scope: vec!["local".to_string()],
            ..DbConfig::for_testing().intent
        };
        let config = DbConfig {
            intent: custom_intent.clone(),
            ..DbConfig::for_testing()
        };
        let handle = open_db_with_config(&dir, &config).expect("open db");
        let snapshot_id = snapshot_start(handle).expect("start snapshot");
        restore_snapshot(handle, snapshot_id).expect("restore snapshot");

        let reopened = with_engine(handle, |engine| Ok(engine.reopen_config_for_restore()))
            .expect("reopen config");
        assert_eq!(reopened.intent, custom_intent);
        assert!(close_db(handle));
    }

    #[test]
    fn snapshot_restore_fails_closed_for_quorum_mode_and_restarts_lanes() {
        let dir = temp_dir();
        let config = DbConfig::for_testing().with_replication(config::ReplicationConfig {
            factor: 3,
            write_quorum: 2,
            ..DbConfig::for_testing().replication
        });
        let handle = open_db_with_config(&dir, &config).expect("open db");
        submit_put(
            handle,
            b"core".to_vec(),
            b"k-snapshot-guard".to_vec(),
            b"v1".to_vec(),
            None,
        )
        .expect("seed value");
        let snapshot_id = snapshot_start(handle).expect("start snapshot");
        let err = restore_snapshot(handle, snapshot_id).expect_err("restore should fail closed");
        assert_eq!(err.code, ErrorCode::InvalidArgument);
        assert!(err.message.contains("SNAPSHOT_RESTORE_SINGLE_NODE_ONLY"));

        submit_put(
            handle,
            b"core".to_vec(),
            b"k-snapshot-guard-after".to_vec(),
            b"v2".to_vec(),
            None,
        )
        .expect("write after failed restore");
        assert_eq!(
            read_point(handle, b"core".to_vec(), b"k-snapshot-guard-after".to_vec())
                .expect("read after failed restore"),
            Some(b"v2".to_vec())
        );
        assert!(close_db(handle));
    }

    #[test]
    fn hlc_state_persists_across_restarts_without_regression() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let txn_id = txn_begin(handle).expect("txn begin");
        txn_prepare(handle, txn_id).expect("txn prepare");

        let before_close = {
            let db = db_for_handle(handle).expect("db handle");
            let engine = db.write().expect("DB engine lock");
            engine.clock_packed()
        };
        assert!(close_db(handle));

        let reopened = open_db(&dir).expect("reopen");
        let after_reopen = {
            let db = db_for_handle(reopened).expect("db handle");
            let engine = db.write().expect("DB engine lock");
            engine.clock_packed()
        };
        assert!(
            after_reopen >= before_close,
            "clock regressed across restart: before={before_close}, after={after_reopen}"
        );
        assert!(close_db(reopened));
    }

    #[test]
    fn raft_membership_state_persists_across_restart() {
        let dir = temp_dir();
        let handle = open_with_test_options(&dir, "ord", 1, None, vec!["ord"]).expect("open db");
        membership_set_voters(handle, vec![1, 2, 3]).expect("set voters");
        membership_begin_joint_change(handle, MembershipChange::AddVoter { node_id: 4 }, 17)
            .expect("begin joint");
        assert!(close_db(handle));

        let reopened = open_with_test_options(&dir, "ord", 1, None, vec!["ord"]).expect("reopen");
        let db = db_for_handle(reopened).expect("db handle");
        let engine = db.write().expect("DB engine lock");
        assert_eq!(
            engine.primary_replication().membership.voters(),
            &BTreeSet::from([1, 2, 3])
        );
        let joint = engine
            .primary_replication()
            .membership
            .joint()
            .expect("joint config restored");
        assert_eq!(joint.outgoing_voters, BTreeSet::from([1, 2, 3]));
        assert_eq!(joint.incoming_voters, BTreeSet::from([1, 2, 3, 4]));
        assert_eq!(joint.started_at_log_index, 17);
        drop(engine);
        assert!(close_db(reopened));
    }

    #[test]
    fn raft_committed_log_tail_persists_across_restart() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        submit_put(
            handle,
            b"core".to_vec(),
            b"k1".to_vec(),
            b"v1".to_vec(),
            None,
        )
        .expect("put");
        submit_put(
            handle,
            b"core".to_vec(),
            b"k2".to_vec(),
            b"v2".to_vec(),
            None,
        )
        .expect("put");
        assert!(close_db(handle));

        let reopened = open_db(&dir).expect("reopen");
        let db = db_for_handle(reopened).expect("db handle");
        let engine = db.write().expect("DB engine lock");
        let leader = &engine.primary_replication().leader;
        assert!(leader.current_term >= 1);
        assert!(leader.last_log_index() >= 2);
        assert!(leader.commit_index <= leader.last_log_index());
        assert_eq!(engine.raft_last_log_index, leader.last_log_index());
        assert_eq!(engine.raft_last_committed_index, leader.commit_index);
        assert_eq!(engine.raft_current_term, leader.current_term);
        drop(engine);
        assert!(close_db(reopened));
    }

    #[test]
    fn close_db_flushes_deferred_raft_persistence() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        {
            let db = db_for_handle(handle).expect("db handle");
            let mut engine = db.write().expect("DB engine lock");
            engine.raft_persist_interval_ops = 10_000;
            engine.raft_persist_ops_since_flush = 0;
        }
        submit_put(
            handle,
            b"core".to_vec(),
            b"k1".to_vec(),
            b"v1".to_vec(),
            None,
        )
        .expect("put");
        assert!(close_db(handle), "close should flush deferred raft state");

        let reopened = open_db(&dir).expect("reopen");
        let db = db_for_handle(reopened).expect("db handle");
        let engine = db.write().expect("DB engine lock");
        assert!(
            engine.primary_replication().leader.last_log_index() >= 1,
            "reopen should observe committed log persisted during close flush"
        );
        drop(engine);
        assert!(close_db(reopened));
    }

    #[test]
    fn close_db_forces_wal_flush_barrier_stats() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        submit_put(
            handle,
            b"core".to_vec(),
            b"wal-close-k".to_vec(),
            b"wal-close-v".to_vec(),
            None,
        )
        .expect("put");
        let before = db_wal_flush_stats(handle).expect("wal stats before close");
        assert_eq!(before.forced_flushes_on_close, 0);
        with_engine_mut(handle, |engine| engine.flush_durable_state()).expect("flush durable");
        let after = db_wal_flush_stats(handle).expect("wal stats after flush");
        assert!(
            after.forced_flushes_on_close >= 1,
            "close-path WAL barrier should be counted"
        );
        assert!(close_db(handle), "close should force WAL flush barrier");
    }

    #[test]
    fn raft_durable_state_corruption_fails_open_fail_closed() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        submit_put(
            handle,
            b"core".to_vec(),
            b"persist".to_vec(),
            b"v".to_vec(),
            None,
        )
        .expect("commit");
        assert!(close_db(handle));

        let raft_state_path =
            crate::db::raft::persistence::raft_state_path_from(&wal_path_from(&dir));
        std::fs::write(&raft_state_path, br#"{"schema_version":"broken"}"#)
            .expect("corrupt raft state");
        let err = open_db(&dir).expect_err("corrupt raft state must fail closed");
        assert_eq!(err.code, ErrorCode::Io);
    }

    #[test]
    fn close_db_removes_handle_even_when_clock_flush_fails() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        std::fs::remove_dir_all(&dir).expect("remove backing dir");
        assert!(
            !close_db(handle),
            "close should report false when flush fails"
        );
        let err = read_point(handle, b"core".to_vec(), b"k".to_vec())
            .expect_err("handle must be removed even on flush failure");
        assert_eq!(err.code, ErrorCode::InvalidArgument);
    }

    #[test]
    fn poisoned_engine_lock_returns_typed_error_instead_of_panicking() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let db = db_for_handle(handle).expect("db handle");
        let poison_target = db.clone();
        let _ = std::panic::catch_unwind(move || {
            let _guard = poison_target.write().expect("lock");
            panic!("poison engine lock");
        });

        let err = submit_put(handle, b"core".to_vec(), b"k".to_vec(), b"v".to_vec(), None)
            .expect_err("poisoned lock should be mapped to io error");
        assert_eq!(err.code, ErrorCode::Io);
        assert!(err.message.contains("lock poisoned"));
        let _ = close_db(handle);
    }

    #[test]
    fn replication_converges_for_conflict_distance_beyond_legacy_cap() {
        let mut leader = NodeState::with_timing(1, 0, 10);
        leader.current_term = 9;
        for index in 1..=40 {
            leader
                .append_log_entry_checked(LogEntry {
                    index,
                    term: 1,
                    payload: vec![index as u8],
                })
                .expect("contiguous append");
        }
        leader.commit_index = leader.last_log_index();

        let mut follower = NodeState::with_timing(2, 0, 10);
        follower.current_term = 9;
        for index in 1..=40 {
            follower
                .append_log_entry_checked(LogEntry {
                    index,
                    term: index,
                    payload: b"stale".to_vec(),
                })
                .expect("contiguous append");
        }

        let response = replicate_to_follower(&leader, &mut follower, leader.commit_index)
            .expect("must converge without fixed 16-attempt cap");
        assert!(response.success);
        assert_eq!(follower.last_log_index(), leader.last_log_index());
        assert_eq!(follower.log_term_at(40), Some(1));
    }

    #[test]
    fn replication_no_progress_returns_retryable_limit_error() {
        let mut leader = NodeState::with_timing(1, 0, 10);
        leader.current_term = 2;
        leader.log = vec![
            LogEntry {
                index: 1,
                term: 2,
                payload: b"a".to_vec(),
            },
            LogEntry {
                index: 3,
                term: 2,
                payload: b"gap".to_vec(),
            },
        ];
        leader.commit_index = 1;

        let mut follower = NodeState::with_timing(2, 0, 10);
        follower.current_term = 2;
        let err = replicate_to_follower(&leader, &mut follower, 1).expect_err("must fail");
        assert_eq!(err.code, ErrorCode::LimitExceeded);
        assert!(err.message.contains("RETRY_AFTER_MS=25"));
    }

    #[test]
    fn cdc_persist_failure_surfaces_and_clears_health() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        submit_put(
            handle,
            b"core".to_vec(),
            b"k1".to_vec(),
            b"v1".to_vec(),
            None,
        )
        .expect("put");

        {
            let db = db_for_handle(handle).expect("db handle");
            let mut engine = db.write().expect("DB engine lock");
            engine.inject_cdc_checkpoint_persist_failure();
        }
        let err = cdc_ack(handle, "orders".to_string(), 1).expect_err("injected fsync failure");
        assert_eq!(err.code, ErrorCode::Io);
        let health = db_health_status(handle).expect("health");
        assert!(health.cdc_checkpoint_persist_error.is_some());
        assert!(health.cdc_checkpoint_persist_error_at.is_some());

        cdc_ack(handle, "orders".to_string(), 1).expect("retry ack");
        let cleared = db_health_status(handle).expect("health");
        assert!(cleared.cdc_checkpoint_persist_error.is_none());
        assert!(cleared.cdc_checkpoint_persist_error_at.is_none());
        assert!(close_db(handle));
    }

    #[test]
    fn uncertainty_window_is_queryable_and_ordered() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let db = db_for_handle(handle).expect("db handle");
        let engine = db.write().expect("DB engine lock");
        let window = engine.uncertainty_window();
        assert!(window.upper_bound >= window.lower_bound);
        drop(engine);
        assert!(close_db(handle));
    }

    #[test]
    fn open_migrates_legacy_raft_state_into_topology_state() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        submit_put(
            handle,
            b"core".to_vec(),
            b"k1".to_vec(),
            b"v1".to_vec(),
            None,
        )
        .expect("put");
        assert!(close_db(handle));

        let wal_path = wal_path_from(&dir);
        let topology_path = crate::db::topology::persistence::topology_state_path_from(&wal_path);
        if topology_path.exists() {
            std::fs::remove_file(&topology_path).expect("remove topology state");
        }
        assert!(
            crate::db::raft::persistence::raft_state_path_from(&wal_path).exists(),
            "legacy raft state should exist for migration fallback"
        );

        let reopened = open_db(&dir).expect("reopen legacy");
        assert!(
            topology_path.exists(),
            "reopen should write topology_state.json"
        );
        let persisted = crate::db::topology::persistence::load_persisted_topology_state(&wal_path)
            .expect("load topology")
            .expect("persisted topology");
        assert!(!persisted.shard_directory.shards.is_empty());
        assert!(persisted.shard_directory.active_group_count >= 1);
        assert!(
            persisted.groups.iter().any(|group| group.group_id == 0),
            "persisted topology must include primary group"
        );
        assert!(close_db(reopened));
    }

    #[test]
    fn topology_state_persists_shard_directory_and_group_state() {
        let dir = temp_dir();
        let config = DbConfig::for_testing()
            .with_replication(config::ReplicationConfig {
                log_backend: ReplicatedLogBackend::DualWal,
                ..DbConfig::for_testing().replication
            })
            .with_topology(config::TopologyConfig {
                initial_logical_shards: 8,
                initial_active_groups: 2,
                local_region: "ord".to_string(),
                region_az_node_map: collapsed_topology_map_for("ord"),
                ..Default::default()
            });
        let handle = open_db_with_config(&dir, &config).expect("open db");
        let before_epoch = shard_map_epoch(handle).expect("shard epoch");
        split_logical_shard(handle, 0).expect("split shard");
        let after_epoch = shard_map_epoch(handle).expect("shard epoch");
        assert!(after_epoch > before_epoch);
        assert!(close_db(handle));

        let reopened = open_db(&dir).expect("reopen");
        assert_eq!(active_group_count(reopened).expect("active groups"), 2);
        assert_eq!(logical_shard_count(reopened).expect("logical shards"), 9);
        assert!(shard_map_epoch(reopened).expect("epoch") >= after_epoch);
        assert!(close_db(reopened));
    }

    #[test]
    fn autoscale_tick_grows_active_groups_toward_membership_target() {
        let dir = temp_dir();
        let config = DbConfig::for_testing()
            .with_replication(config::ReplicationConfig {
                factor: 3,
                write_quorum: 2,
                log_backend: ReplicatedLogBackend::DualWal,
                ..DbConfig::for_testing().replication
            })
            .with_topology(config::TopologyConfig {
                initial_logical_shards: 16,
                autoscale_enabled: true,
                autoscale_tick_ms: 60_000,
                local_region: "ord".to_string(),
                region_az_node_map: collapsed_topology_map_for("ord"),
                ..Default::default()
            });
        let handle = open_db_with_config(&dir, &config).expect("open db");
        assert_eq!(active_group_count(handle).expect("active groups"), 1);

        let first = autoscale_tick(handle).expect("tick");
        assert_eq!(first.last_action, "grow_group");
        assert_eq!(active_group_count(handle).expect("active groups"), 2);

        let second = autoscale_tick(handle).expect("tick");
        assert_eq!(second.last_action, "grow_group");
        assert_eq!(active_group_count(handle).expect("active groups"), 3);

        assert!(close_db(handle));
    }

    #[test]
    fn autoscale_tick_splits_hottest_shard_when_skew_exceeds_threshold() {
        let dir = temp_dir();
        let config = DbConfig::for_testing()
            .with_replication(config::ReplicationConfig {
                log_backend: ReplicatedLogBackend::DualWal,
                ..DbConfig::for_testing().replication
            })
            .with_topology(config::TopologyConfig {
                initial_logical_shards: 8,
                initial_active_groups: 3,
                autoscale_enabled: true,
                autoscale_tick_ms: 60_000,
                autoscale_max_skew_ratio: 1.2,
                local_region: "ord".to_string(),
                region_az_node_map: collapsed_topology_map_for("ord"),
                ..Default::default()
            });
        let handle = open_db_with_config(&dir, &config).expect("open db");
        let before = logical_shard_count(handle).expect("logical shards");
        let hot_shard =
            shard_for_key(handle, b"core".to_vec(), b"hot-key".to_vec()).expect("hot shard route");
        {
            let db = db_for_handle(handle).expect("db handle");
            let mut engine = db.write().expect("DB engine lock");
            engine.shard_write_ops.insert(hot_shard, 32);
        }

        let status = autoscale_tick(handle).expect("tick");
        assert_eq!(status.last_action, "split_shard");
        let after = logical_shard_count(handle).expect("logical shards");
        assert!(after > before, "autoscale must split hot shard");
        assert!(close_db(handle));
    }

    #[test]
    fn autoscale_tick_blocks_when_discovered_nodes_cannot_meet_quorum() {
        let dir = temp_dir();
        let config = DbConfig::for_testing()
            .with_replication(config::ReplicationConfig {
                factor: 3,
                write_quorum: 3,
                log_backend: ReplicatedLogBackend::DualWal,
                ..DbConfig::for_testing().replication
            })
            .with_topology(config::TopologyConfig {
                initial_logical_shards: 8,
                autoscale_enabled: true,
                autoscale_tick_ms: 60_000,
                local_region: "ord".to_string(),
                region_az_node_map: collapsed_topology_map_for("ord"),
                ..Default::default()
            });
        let handle = open_db_with_config(&dir, &config).expect("open db");

        {
            let db = db_for_handle(handle).expect("db handle");
            let mut engine = db.write().expect("DB engine lock");
            engine.inject_autoscale_healthy_nodes(vec![1, 2]);
        }

        let status = autoscale_tick(handle).expect("tick");
        assert_eq!(status.last_action, "blocked");
        assert!(
            status
                .reasons
                .iter()
                .any(|reason| reason.contains("quorum simulation failed")),
            "expected quorum simulation failure reason, got {:?}",
            status.reasons
        );
        assert_eq!(active_group_count(handle).expect("active groups"), 1);
        assert!(close_db(handle));
    }

    #[test]
    fn autoscale_tick_uses_node_inventory_growth_and_keeps_grow_only_behavior() {
        let dir = temp_dir();
        let config = DbConfig::for_testing()
            .with_replication(config::ReplicationConfig {
                factor: 3,
                write_quorum: 2,
                log_backend: ReplicatedLogBackend::DualWal,
                ..DbConfig::for_testing().replication
            })
            .with_topology(config::TopologyConfig {
                initial_logical_shards: 8,
                autoscale_enabled: true,
                autoscale_tick_ms: 60_000,
                local_region: "ord".to_string(),
                region_az_node_map: collapsed_topology_map_for("ord"),
                ..Default::default()
            });
        let handle = open_db_with_config(&dir, &config).expect("open db");

        {
            let db = db_for_handle(handle).expect("db handle");
            let mut engine = db.write().expect("DB engine lock");
            engine.inject_autoscale_healthy_nodes(vec![1, 2, 3]);
        }

        let first = autoscale_tick(handle).expect("tick");
        assert_eq!(first.last_action, "grow_group");
        assert_eq!(active_group_count(handle).expect("active groups"), 2);
        let second = autoscale_tick(handle).expect("tick");
        assert_eq!(second.last_action, "grow_group");
        assert_eq!(active_group_count(handle).expect("active groups"), 3);

        {
            let db = db_for_handle(handle).expect("db handle");
            let mut engine = db.write().expect("DB engine lock");
            engine.inject_autoscale_healthy_nodes(vec![1, 2, 3, 4, 5]);
        }

        let expected_voters = vec![
            vec![1, 2, 3],
            vec![2, 3, 4],
            vec![3, 4, 5],
            vec![1, 4, 5],
            vec![1, 2, 5],
        ];
        let mut converged = false;
        for _ in 0..24 {
            let _ = autoscale_tick(handle).expect("tick");
            let topology = topology_status(handle).expect("topology");
            if topology.active_groups != 5 {
                continue;
            }
            let mut matches = true;
            for group_id in 0..5u32 {
                let Some(group) = topology
                    .groups
                    .iter()
                    .find(|group| group.group_id == group_id)
                else {
                    matches = false;
                    break;
                };
                if group.voters != expected_voters[group_id as usize] {
                    matches = false;
                    break;
                }
            }
            if matches {
                converged = true;
                break;
            }
        }
        assert!(
            converged,
            "autoscale should converge to deterministic rf=3 replica sets across 5 groups"
        );

        {
            let db = db_for_handle(handle).expect("db handle");
            let mut engine = db.write().expect("DB engine lock");
            engine.inject_autoscale_healthy_nodes(vec![1, 2]);
        }
        for _ in 0..8 {
            let _ = autoscale_tick(handle).expect("tick");
        }
        assert_eq!(
            active_group_count(handle).expect("active groups"),
            5,
            "grow-only mode must not downscale groups"
        );

        assert!(close_db(handle));
    }

    #[test]
    fn autopilot_boot_tick_populates_explainable_state() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let effective = intent_effective(handle).expect("intent effective");
        assert_eq!(effective.mode, "full_auto");
        assert!(effective.replication_factor >= 1);

        let actions = autopilot_last_actions(handle, 4).expect("actions");
        assert!(!actions.is_empty(), "boot tick should record audit row");
        assert_eq!(actions[0].source, "boot");
        assert!(close_db(handle));
    }

    #[test]
    fn autopilot_orchestration_fails_closed_for_unsafe_actions() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        with_engine_mut(handle, |engine| {
            engine.replication_factor = 1;
            engine.write_quorum = 3;
            engine.run_autopilot_controller_tick("test-unsafe");
            Ok(())
        })
        .expect("mutate engine");

        let conflicts = intent_conflicts(handle).expect("conflicts");
        assert!(
            conflicts.iter().any(|conflict| conflict.blocking),
            "unsafe action should emit blocking conflict"
        );

        let actions = autopilot_last_actions(handle, 1).expect("actions");
        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0].final_reason_code.as_str(),
            crate::db::autopilot::orchestrator::ActionReasonCode::UnsafeActionFailClosed.as_str()
        );
        assert!(close_db(handle));
    }

    #[test]
    fn topology_restart_preserves_per_group_replica_sets_after_autoscale_growth() {
        let dir = temp_dir();
        let config = DbConfig::for_testing()
            .with_replication(config::ReplicationConfig {
                factor: 3,
                write_quorum: 2,
                log_backend: ReplicatedLogBackend::DualWal,
                ..DbConfig::for_testing().replication
            })
            .with_topology(config::TopologyConfig {
                initial_logical_shards: 8,
                autoscale_enabled: true,
                autoscale_tick_ms: 60_000,
                local_region: "ord".to_string(),
                region_az_node_map: collapsed_topology_map_for("ord"),
                ..Default::default()
            });
        let handle = open_db_with_config(&dir, &config).expect("open db");
        {
            let db = db_for_handle(handle).expect("db handle");
            let mut engine = db.write().expect("DB engine lock");
            engine.inject_autoscale_healthy_nodes(vec![1, 2, 3, 4, 5]);
        }
        for _ in 0..24 {
            let _ = autoscale_tick(handle).expect("tick");
            if active_group_count(handle).expect("group count") >= 5 {
                break;
            }
        }
        let before = topology_status(handle).expect("topology before");
        assert_eq!(
            before.active_groups, 5,
            "expected autoscale to grow to 5 groups"
        );
        assert!(close_db(handle));

        let reopened = open_db(&dir).expect("reopen");
        let after = topology_status(reopened).expect("topology after");
        assert_eq!(after.active_groups, before.active_groups);
        assert_eq!(after.logical_shards, before.logical_shards);
        for group in &before.groups {
            let restored = after
                .groups
                .iter()
                .find(|candidate| candidate.group_id == group.group_id)
                .expect("group exists after restart");
            assert_eq!(restored.voters, group.voters);
        }
        assert!(close_db(reopened));
    }

    #[test]
    fn canonical_only_backend_roundtrip_reports_health_mode() {
        let dir = temp_dir();
        let config = DbConfig::for_testing().with_topology(config::TopologyConfig {
            local_region: "ord".to_string(),
            region_az_node_map: collapsed_topology_map_for("ord"),
            ..Default::default()
        });
        let handle = open_db_with_config(&dir, &config).expect("open db");
        submit_put(
            handle,
            b"core".to_vec(),
            b"k-canonical".to_vec(),
            b"v-canonical".to_vec(),
            None,
        )
        .expect("put");
        let value = read_point(handle, b"core".to_vec(), b"k-canonical".to_vec())
            .expect("read")
            .expect("value");
        assert_eq!(value, b"v-canonical".to_vec());
        let health = db_health_status(handle).expect("health");
        assert_eq!(
            health.replicated_log_backend,
            ReplicatedLogBackend::CanonicalOnly
        );
        assert!(close_db(handle));
    }

    #[test]
    fn canonical_only_persists_raft_metadata_outside_wal_records() {
        let dir = temp_dir();
        let handle = open_with_backend(&dir, ReplicatedLogBackend::CanonicalOnly)
            .expect("open canonical backend");
        {
            let db = db_for_handle(handle).expect("db handle");
            let mut engine = db.write().expect("DB engine lock");
            engine.raft_persist_interval_ops = 1;
            engine.raft_persist_ops_since_flush = 0;
        }
        submit_put(
            handle,
            b"core".to_vec(),
            b"k-canonical-meta".to_vec(),
            b"v-canonical-meta".to_vec(),
            None,
        )
        .expect("put");
        assert!(close_db(handle));

        let wal_path = wal_path_from(&dir);
        let segment = WalSegment::open(&wal_path).expect("open wal");
        let records = recover(&segment).expect("recover wal records");
        assert!(
            !records
                .iter()
                .any(|record| matches!(record.kind, RecordKind::RaftMeta)),
            "canonical-only backend should persist raft metadata out-of-band"
        );
        let metadata = load_raft_metadata_binary(&wal_path)
            .expect("load binary raft metadata")
            .expect("binary raft metadata should exist");
        assert!(
            metadata.commit_index >= 1,
            "canonical metadata commit index should advance after write"
        );
    }

    #[test]
    fn replicated_log_backends_recover_same_user_data() {
        for backend in [
            ReplicatedLogBackend::DualWal,
            ReplicatedLogBackend::ShadowCanonical,
            ReplicatedLogBackend::CanonicalOnly,
        ] {
            let dir = temp_dir();
            let handle = open_with_backend(&dir, backend).expect("open backend");
            submit_put(
                handle,
                b"core".to_vec(),
                format!("k-backend-{backend:?}").into_bytes(),
                format!("v-backend-{backend:?}").into_bytes(),
                None,
            )
            .expect("put");
            assert!(close_db(handle));

            let reopened = open_with_backend(&dir, backend).expect("reopen backend");
            let key = format!("k-backend-{backend:?}").into_bytes();
            let value = read_point(reopened, b"core".to_vec(), key)
                .expect("read")
                .expect("value");
            assert_eq!(value, format!("v-backend-{backend:?}").into_bytes());
            let health = db_health_status(reopened).expect("health");
            assert_eq!(health.replicated_log_backend, backend);
            assert!(close_db(reopened));
        }
    }

    #[test]
    fn backend_rollback_path_recovers_across_modes() {
        let dir = temp_dir();
        let dual =
            open_with_backend(&dir, ReplicatedLogBackend::DualWal).expect("open dual backend");
        submit_put(
            dual,
            b"core".to_vec(),
            b"k-rollback".to_vec(),
            b"v-rollback".to_vec(),
            None,
        )
        .expect("put in dual backend");
        assert!(close_db(dual));

        let canonical = open_with_backend(&dir, ReplicatedLogBackend::CanonicalOnly)
            .expect("reopen canonical backend");
        let canonical_value = read_point(canonical, b"core".to_vec(), b"k-rollback".to_vec())
            .expect("read canonical")
            .expect("canonical value");
        assert_eq!(canonical_value, b"v-rollback".to_vec());
        assert!(close_db(canonical));

        let shadow = open_with_backend(&dir, ReplicatedLogBackend::ShadowCanonical)
            .expect("reopen shadow backend");
        let shadow_value = read_point(shadow, b"core".to_vec(), b"k-rollback".to_vec())
            .expect("read shadow")
            .expect("shadow value");
        assert_eq!(shadow_value, b"v-rollback".to_vec());
        assert!(close_db(shadow));
    }

    #[test]
    fn insert_fast_lane_tracks_accept_and_fallback_rejection() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        submit_put_insert_fast(
            handle,
            b"core".to_vec(),
            b"fast-k1".to_vec(),
            b"fast-v1".to_vec(),
        )
        .expect("fast put");
        let fallback_version = submit_batch_insert_fast(
            handle,
            &[BatchOp::Delete {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"fast-k1"),
                expected_version: None,
            }],
        )
        .expect("fallback batch");
        assert!(fallback_version >= 1);
        let health = db_health_status(handle).expect("health");
        assert!(health.insert_fast_lane_active);
        assert!(health.insert_fast_lane_accepted >= 1);
        assert!(health.insert_fast_lane_rejected >= 1);
        assert!(close_db(handle));
    }

    #[test]
    fn outside_lock_prepare_replicate_finalize_roundtrip_preserves_quorum_visibility() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let db = db_for_handle(handle).expect("db handle");

        let mut engine = db.write().expect("DB engine lock");
        let batch = vec![BatchOp::Put {
            namespace: Bytes::from_static(b"core"),
            key: Bytes::from_static(b"outside-lock-k"),
            value: Bytes::from_static(b"outside-lock-v"),
            expected_version: None,
        }];
        engine
            .sync_keyrange_ownership_state()
            .expect("sync ownership");
        let route = engine.route_batch_to_shard(&batch).expect("route batch");
        let fence = engine
            .current_ownership_fence_for_route(&route)
            .expect("ownership fence");
        let preprocessed = preprocess_batch(&batch).expect("preprocess batch");
        let prepared = engine
            .prepare_batch_for_outside_replication(&batch, preprocessed, 0, 0, &fence)
            .expect("prepare outside-lock batch");
        let required_term = prepared.required_term;
        let required_index = prepared.required_index;
        let fanout = OutsideLockFanoutResult {
            replicate_ns: 1,
            used_simulation: false,
            sorted_run_chunks_sent: 2,
            total_target_count: 2,
            contacted_target_count: 1,
            replication_wave_count: 1,
            replication_wave_total_targets: 1,
            replication_wave_max_targets: 1,
            successful_target_count: 1,
            failed_target_count: 0,
            cancelled_target_count: 0,
            aborted_in_flight_count: 0,
            follower_responses: vec![FollowerAppendResponse {
                node_id: 2,
                response: AppendEntriesResponse {
                    term: required_term,
                    success: true,
                    match_index: required_index,
                    conflict_index: None,
                },
                replication_latency_ns: 1,
                fsync_latency_ns: 1,
            }],
            follower_state_updates: Vec::new(),
            follower_progress_updates: Vec::new(),
            replication_error: None,
        };
        let mut result = engine
            .finalize_prepared_batch_after_outside_replication(prepared, Ok(fanout))
            .expect("finalize outside-lock batch");
        engine.submit_wal_and_record_stage(&mut result);
        engine.mark_group_durable(result.active_group_id, result.required_index);
        if result.staged_ops.is_empty() {
            engine.mark_group_apply_visible(result.active_group_id, result.required_index);
        } else {
            engine.apply_staged_ops(&result.staged_ops);
            engine.mark_group_apply_visible(result.active_group_id, result.required_index);
            result.staged_ops.clear();
        }
        drop(engine);

        let value = read_point(handle, b"core".to_vec(), b"outside-lock-k".to_vec())
            .expect("read point")
            .expect("existing value");
        assert_eq!(value, b"outside-lock-v".to_vec());
        let health = db_health_status(handle).expect("health");
        assert!(
            health.sorted_run_catchup_chunks_sent >= 2,
            "fanout finalize should roll sorted-run sender telemetry into health"
        );
        assert!(close_db(handle));
    }

    #[test]
    fn wal_encode_and_frontier_defaults_surface_in_health_and_execute() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        submit_put(
            handle,
            b"core".to_vec(),
            b"defaults-k".to_vec(),
            b"defaults-v".to_vec(),
            None,
        )
        .expect("put");
        let health = db_health_status(handle).expect("health");
        assert!(health.wal_encode_outside_lock_active);
        assert!(health.replication_outside_lock_active);
        assert!(health.latency_frontier_mode_active);
        assert_eq!(
            health.sorted_run_catchup_lag_threshold_ops,
            SORTED_RUN_CATCHUP_LAG_THRESHOLD_OPS
        );
        assert!(
            health.frontier_speculative_plans >= 1,
            "frontier mode should record speculative planning telemetry"
        );
        assert!(close_db(handle));
    }

    #[test]
    fn sorted_run_sender_chunk_builder_is_deterministic_and_chunked() {
        let large = vec![7u8; 200 * 1024];
        let records = vec![
            Record {
                kind: RecordKind::Put,
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"c"),
                value: Bytes::from(large.clone()),
                version: 7,
            },
            Record {
                kind: RecordKind::Put,
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"b"),
                value: Bytes::from(large),
                version: 8,
            },
            Record {
                kind: RecordKind::Delete,
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"a"),
                value: Bytes::new(),
                version: 9,
            },
        ];
        let first = build_sorted_run_chunk_payloads_from_records(&records);
        let second = build_sorted_run_chunk_payloads_from_records(&records);
        assert_eq!(first, second, "chunk payloads should be deterministic");
        assert!(
            first.len() >= 2,
            "large payloads should force chunk split at default chunk limits"
        );

        let mut flattened = Vec::new();
        for chunk in &first {
            flattened.extend(crate::db::lsm::sstable::decode_block(chunk).expect("decode chunk"));
        }
        let key_a = encode_user_key(b"core", b"a").expect("encode a");
        let key_b = encode_user_key(b"core", b"b").expect("encode b");
        let key_c = encode_user_key(b"core", b"c").expect("encode c");
        assert_eq!(flattened.len(), 3);
        assert_eq!(flattened[0].key, key_a);
        assert!(flattened[0].is_tombstone());
        assert_eq!(flattened[1].key, key_b);
        assert_eq!(flattened[2].key, key_c);
    }

    #[test]
    fn sorted_run_chunk_rejects_stale_term() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let user_key = encode_user_key(b"core", b"stale").expect("encode user key");
        let payload =
            crate::db::lsm::sstable::encode_block(&[crate::db::lsm::sstable::SsTableEntry::live(
                user_key,
                9,
                b"v".to_vec(),
                None,
            )]);
        let status = replica_install_sorted_run_chunk(handle, 0, 100, 0, 1, payload)
            .expect("sorted-run call");
        assert!(!status.accepted);
        assert_eq!(
            status.rejection_reason.as_deref(),
            Some("SORTED_RUN_STALE_TERM_REJECTED")
        );
        assert!(close_db(handle));
    }

    #[test]
    fn sorted_run_chunk_duplicate_replay_and_convergence() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");

        let key1 = encode_user_key(b"core", b"k-sorted-1").expect("encode key1");
        let key2 = encode_user_key(b"core", b"k-sorted-2").expect("encode key2");
        let chunk0 =
            crate::db::lsm::sstable::encode_block(&[crate::db::lsm::sstable::SsTableEntry::live(
                key1,
                11,
                b"v1".to_vec(),
                None,
            )]);
        let chunk1 =
            crate::db::lsm::sstable::encode_block(&[crate::db::lsm::sstable::SsTableEntry::live(
                key2,
                12,
                b"v2".to_vec(),
                None,
            )]);

        let first = replica_install_sorted_run_chunk(handle, 1, 200, 0, 2, chunk0.clone())
            .expect("first chunk");
        assert!(first.accepted);
        assert_eq!(first.next_chunk_index, 1);

        let duplicate = replica_install_sorted_run_chunk(handle, 1, 200, 0, 2, chunk0.clone())
            .expect("duplicate chunk");
        assert!(duplicate.accepted);
        assert_eq!(duplicate.next_chunk_index, 1);

        let mismatch_payload =
            crate::db::lsm::sstable::encode_block(&[crate::db::lsm::sstable::SsTableEntry::live(
                encode_user_key(b"core", b"k-sorted-1").expect("encode mismatch key"),
                13,
                b"bad".to_vec(),
                None,
            )]);
        let mismatch = replica_install_sorted_run_chunk(handle, 1, 200, 0, 2, mismatch_payload)
            .expect("mismatch chunk");
        assert!(!mismatch.accepted);
        assert_eq!(
            mismatch.rejection_reason.as_deref(),
            Some("SORTED_RUN_DUPLICATE_CHUNK_PAYLOAD_MISMATCH")
        );

        let second =
            replica_install_sorted_run_chunk(handle, 1, 200, 1, 2, chunk1).expect("second chunk");
        assert!(second.accepted);
        assert_eq!(second.next_chunk_index, 2);

        let v1 = read_point(handle, b"core".to_vec(), b"k-sorted-1".to_vec())
            .expect("read key1")
            .expect("key1 value");
        let v2 = read_point(handle, b"core".to_vec(), b"k-sorted-2".to_vec())
            .expect("read key2")
            .expect("key2 value");
        assert_eq!(v1, b"v1".to_vec());
        assert_eq!(v2, b"v2".to_vec());

        let health = db_health_status(handle).expect("health");
        assert!(health.sorted_run_catchup_active);
        assert!(health.sorted_run_catchup_requests >= 4);
        assert!(health.sorted_run_catchup_chunks_applied >= 2);
        assert!(close_db(handle));
    }

    #[test]
    fn sorted_run_chunk_out_of_order_then_replay_restart_converges() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");

        let key1 = encode_user_key(b"core", b"k-sorted-restart-1").expect("encode key1");
        let key2 = encode_user_key(b"core", b"k-sorted-restart-2").expect("encode key2");
        let chunk0 =
            crate::db::lsm::sstable::encode_block(&[crate::db::lsm::sstable::SsTableEntry::live(
                key1,
                21,
                b"v1".to_vec(),
                None,
            )]);
        let chunk1 =
            crate::db::lsm::sstable::encode_block(&[crate::db::lsm::sstable::SsTableEntry::live(
                key2,
                22,
                b"v2".to_vec(),
                None,
            )]);

        let out_of_order =
            replica_install_sorted_run_chunk(handle, 1, 300, 1, 2, chunk1.clone()).expect("oOO");
        assert!(!out_of_order.accepted);
        assert_eq!(
            out_of_order.rejection_reason.as_deref(),
            Some("SORTED_RUN_OUT_OF_ORDER_CHUNK")
        );
        assert_eq!(out_of_order.next_chunk_index, 0);

        let first =
            replica_install_sorted_run_chunk(handle, 1, 300, 0, 2, chunk0.clone()).expect("first");
        assert!(first.accepted);
        assert_eq!(first.next_chunk_index, 1);

        // Simulate sender restart after a partial stream: replaying the already
        // delivered chunk must remain idempotent and continue from chunk #1.
        let replay =
            replica_install_sorted_run_chunk(handle, 1, 300, 0, 2, chunk0).expect("replay first");
        assert!(replay.accepted);
        assert_eq!(replay.next_chunk_index, 1);

        let second = replica_install_sorted_run_chunk(handle, 1, 300, 1, 2, chunk1)
            .expect("second after replay");
        assert!(second.accepted);
        assert_eq!(second.next_chunk_index, 2);

        let v1 = read_point(handle, b"core".to_vec(), b"k-sorted-restart-1".to_vec())
            .expect("read key1")
            .expect("key1 value");
        let v2 = read_point(handle, b"core".to_vec(), b"k-sorted-restart-2".to_vec())
            .expect("read key2")
            .expect("key2 value");
        assert_eq!(v1, b"v1".to_vec());
        assert_eq!(v2, b"v2".to_vec());

        let health = db_health_status(handle).expect("health");
        assert!(health.sorted_run_catchup_requests >= 4);
        assert!(health.sorted_run_catchup_chunks_applied >= 2);
        assert!(close_db(handle));
    }

    #[test]
    fn blob_value_separation_round_trips_point_range_and_cdc() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let large_value = vec![9u8; BLOB_VALUE_THRESHOLD_BYTES + 256];
        submit_put(
            handle,
            b"core".to_vec(),
            b"k-blob".to_vec(),
            large_value.clone(),
            None,
        )
        .expect("put large value");

        let observed = read_point(handle, b"core".to_vec(), b"k-blob".to_vec())
            .expect("read point")
            .expect("value");
        assert_eq!(observed, large_value);

        let rows = read_range(
            handle,
            b"core".to_vec(),
            b"k-blob".to_vec(),
            b"k-blob~".to_vec(),
            4,
        )
        .expect("read range");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, observed);

        let page = cdc_page(handle, 0, 8, None).expect("cdc page");
        let event = page.events.last().expect("cdc event");
        assert_eq!(event.key.as_ref(), b"k-blob");
        assert_eq!(
            event.value.as_ref().map(|value| value.as_ref()),
            Some(observed.as_slice())
        );

        let health = db_health_status(handle).expect("health");
        assert!(health.blob_values_externalized >= 1);
        assert!(close_db(handle));
    }

    #[test]
    fn blob_value_separation_range_iterator_returns_materialized_values() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let large_value = vec![5u8; BLOB_VALUE_THRESHOLD_BYTES + 128];
        submit_put(
            handle,
            b"core".to_vec(),
            b"k-blob-iter".to_vec(),
            large_value.clone(),
            None,
        )
        .expect("put large value");

        let db = db_for_handle(handle).expect("db handle");
        let engine = db.write().expect("DB engine lock");
        let mut iter = engine
            .read_range_iter(
                b"core",
                b"k-blob-iter",
                b"k-blob-iter~",
                8,
                RangeCancellation::new(),
                ReadConsistency::Eventual,
                None,
            )
            .expect("range iter");
        let row = iter.try_next().expect("next row").expect("first row");
        assert_eq!(row.1.to_vec(), large_value);
        assert!(iter.try_next().expect("second row").is_none());
        drop(engine);
        assert!(close_db(handle));
    }

    #[test]
    fn blob_gc_runtime_reclaims_only_unreferenced_after_memtable_gc() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let value_a = vec![1u8; BLOB_VALUE_THRESHOLD_BYTES + 64];
        let value_b = vec![2u8; BLOB_VALUE_THRESHOLD_BYTES + 96];
        submit_put(
            handle,
            b"core".to_vec(),
            b"k-blob-gc-a".to_vec(),
            value_a,
            None,
        )
        .expect("put a");
        submit_put(
            handle,
            b"core".to_vec(),
            b"k-blob-gc-b".to_vec(),
            value_b.clone(),
            None,
        )
        .expect("put b");
        submit_batch(
            handle,
            &[BatchOp::Delete {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"k-blob-gc-a"),
                expected_version: None,
            }],
        )
        .expect("delete a");

        {
            let db = db_for_handle(handle).expect("db handle");
            let mut engine = db.write().expect("DB engine lock");
            let before = engine.blob_store.blob_count();
            assert!(before >= 2, "both large values should be externalized");
            let _ = engine.memtable.gc_old_versions(u64::MAX);
            engine.run_blob_gc_cycle();
            assert_eq!(
                engine.blob_store.blob_count(),
                1,
                "GC should reclaim only blobs no longer referenced by memtable"
            );
        }

        let missing =
            read_point(handle, b"core".to_vec(), b"k-blob-gc-a".to_vec()).expect("read a");
        assert!(missing.is_none());
        let surviving = read_point(handle, b"core".to_vec(), b"k-blob-gc-b".to_vec())
            .expect("read b")
            .expect("value b");
        assert_eq!(surviving, value_b);
        assert!(close_db(handle));
    }

    #[test]
    fn blob_value_separation_replays_after_restart() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let large_value = vec![3u8; BLOB_VALUE_THRESHOLD_BYTES + 512];
        submit_put(
            handle,
            b"core".to_vec(),
            b"k-blob-restart".to_vec(),
            large_value.clone(),
            None,
        )
        .expect("put large value");
        assert!(close_db(handle));

        let reopened = open_db(&dir).expect("reopen db");
        let observed = read_point(reopened, b"core".to_vec(), b"k-blob-restart".to_vec())
            .expect("read point")
            .expect("value");
        assert_eq!(observed, large_value);
        assert!(close_db(reopened));
    }
}
