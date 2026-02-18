use crate::db::cdc::CdcEmitter;
use crate::db::hlc::HybridLogicalClock;
use crate::db::keyspace::{decode_user_key, encode_user_key};
use crate::db::mvcc::memtable::Memtable;
use crate::db::mvcc::occ::validate_expected_version;
use crate::db::raft::append::handle_append_entries;
use crate::db::raft::membership::{MembershipChange, MembershipConfig};
use crate::db::raft::message::{AppendEntries, AppendEntriesResponse, LogEntry};
use crate::db::raft::persistence::{
    PersistedRaftState, load_persisted_raft_state, persist_raft_state,
};
use crate::db::raft::pipeline::{RaftCommand, build_append_frame};
use crate::db::raft::state::NodeState;
use crate::db::read::iterator::{RangeCancellation, RangeIterator};
use crate::db::read::rejection::{StrongReadErrorCode, enforce_strong_read};
use crate::db::read::{ReadPath, ReadPathStats};
use crate::db::replication::ack::{LeaderAckInput, evaluate_leader_ack};
use crate::db::replication::quorum::{FollowerAppendResponse, response_is_durable_ack};
use crate::db::time::persistence::{load_hlc_state, persist_hlc_state};
use crate::db::time::safe_time::{SafeTimeDiagnostics, SafeTimeLagBudget, SafeTimePropagator};
use crate::db::time::uncertainty::{UncertaintyTracker, UncertaintyWindow};
use crate::db::time::watermarks::SafeReadWatermarks;
#[cfg(test)]
use crate::db::txn::lock_table::LockTableSnapshot;
use crate::db::txn::lock_table::{LockAcquireOutcome, TxnLockTable};
use crate::db::types::{
    BatchOp, DbError, MAX_BATCH_BYTES, MAX_BATCH_OPS, MAX_KEY_BYTES, MAX_VALUE_BYTES,
};
use crate::db::wal::format::{Record, RecordKind};
use crate::db::wal::recovery::recover;
use crate::db::wal::segment::WalSegment;
use std::collections::{BTreeSet, HashMap};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

pub mod abi;
pub mod admin_api;
pub mod analytics;
pub mod api;
pub mod audit;
pub mod autopilot;
pub mod backup;
pub mod cdc;
pub mod cluster;
pub mod codec;
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
pub mod scrub;
pub mod security;
pub mod shard;
pub mod snapshot;
pub mod sql;
pub mod tenant;
pub mod time;
pub mod txn;
pub mod types;
pub mod versioning;
pub mod wal;
pub mod writer;

#[derive(Debug)]
pub struct DbEngine {
    memtable: Memtable,
    read_path: ReadPath,
    wal: WalSegment,
    replication: ReplicationState,
    raft_current_term: u64,
    raft_last_log_index: u64,
    raft_last_committed_index: u64,
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
    #[cfg(test)]
    fail_next_cdc_checkpoint_persist: bool,
}

#[derive(Debug, Clone)]
struct ReplicationState {
    leader: NodeState,
    followers: HashMap<u64, NodeState>,
    membership: MembershipConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadConsistency {
    Strong,
    Eventual,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbHealthStatus {
    pub clock_persist_error: Option<String>,
    pub clock_persist_error_at: Option<u64>,
    pub raft_persist_error: Option<String>,
    pub raft_persist_error_at: Option<u64>,
    pub cdc_checkpoint_persist_error: Option<String>,
    pub cdc_checkpoint_persist_error_at: Option<u64>,
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
}

#[derive(Debug, Clone, Copy)]
struct SnapshotRecord {
    created_ts: u64,
    progress: u8,
    restored_ts: Option<u64>,
}

#[derive(Debug, Clone)]
enum StagedApplyOp {
    Put {
        user_key: Vec<u8>,
        cache_key: Vec<u8>,
        namespace: Vec<u8>,
        key: Vec<u8>,
        value: Vec<u8>,
        version: u64,
    },
    Delete {
        user_key: Vec<u8>,
        cache_key: Vec<u8>,
        namespace: Vec<u8>,
        key: Vec<u8>,
        version: u64,
    },
}

const DEFAULT_POINT_READ_IN_FLIGHT_LIMIT: usize = 64;
const DEFAULT_RANGE_READ_IN_FLIGHT_LIMIT: usize = 8;
const DEFAULT_POINT_READ_CACHE_CAPACITY: usize = 1024;
const DEFAULT_NEGATIVE_BLOOM_CAPACITY: usize = 1024;
const LOCAL_NODE_ID: u64 = 1;
const DEFAULT_MAX_CLOCK_SKEW_MS: u64 = 25;
const LOCAL_REGION_ID: &str = "local";

fn now_epoch_s() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|dur| dur.as_secs())
        .unwrap_or(0)
}

impl DbEngine {
    pub fn open(path: &Path) -> Result<Self, DbError> {
        let wal = WalSegment::open(path).map_err(|err| DbError::io(err.to_string()))?;
        let records = recover(&wal).map_err(|err| DbError::io(err.to_string()))?;
        let cdc_checkpoints = load_cdc_checkpoints(path)?;
        let clock = HybridLogicalClock::new();
        let uncertainty = UncertaintyTracker::new(DEFAULT_MAX_CLOCK_SKEW_MS);
        let watermarks = SafeReadWatermarks::new();
        let mut safe_time = SafeTimePropagator::default();
        let mut leader = NodeState::with_timing(LOCAL_NODE_ID, 0, 10);
        leader.current_term = 1;
        let mut membership = MembershipConfig::new([LOCAL_NODE_ID])
            .map_err(|err| DbError::invalid_argument(format!("membership init failed: {err:?}")))?;
        if let Some(persisted) =
            load_persisted_raft_state(path).map_err(|err| DbError::io(err.to_string()))?
        {
            membership = persisted
                .restore(&mut leader, 0, 10)
                .map_err(|err| DbError::io(err.to_string()))?;
        }
        let replication = ReplicationState {
            leader,
            followers: HashMap::new(),
            membership,
        };

        if let Some(persisted) = load_hlc_state(path).map_err(|err| DbError::io(err.to_string()))? {
            clock.observe_packed(persisted);
            uncertainty.observe_remote_packed(persisted);
            watermarks.observe(LOCAL_NODE_ID, persisted);
            safe_time.observe_shard_safe_time("clock", LOCAL_REGION_ID, persisted);
        }

        let mut memtable = Memtable::default();
        for rec in records {
            let user_key = encode_user_key(&rec.namespace, &rec.key)?;
            let value = match rec.kind {
                RecordKind::Put => Some(rec.value),
                RecordKind::Delete => None,
            };
            memtable.apply(user_key, rec.version, value);
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
        let mut engine = Self {
            memtable,
            read_path: ReadPath::new(
                DEFAULT_POINT_READ_IN_FLIGHT_LIMIT,
                DEFAULT_RANGE_READ_IN_FLIGHT_LIMIT,
                DEFAULT_POINT_READ_CACHE_CAPACITY,
                DEFAULT_NEGATIVE_BLOOM_CAPACITY,
            ),
            wal,
            replication,
            raft_current_term: 1,
            raft_last_log_index: 0,
            raft_last_committed_index: 0,
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
            #[cfg(test)]
            fail_next_cdc_checkpoint_persist: false,
        };
        engine.raft_current_term = engine.replication.leader.current_term.max(1);
        engine.raft_last_log_index = engine.replication.leader.last_log_index();
        engine.raft_last_committed_index = engine.replication.leader.commit_index;
        engine.refresh_replication_followers();
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

    fn persist_clock_state_best_effort(&mut self, packed: u64) {
        if let Err(err) = self.persist_clock_state(packed) {
            self.clock_persist_error = Some(err.message);
            self.clock_persist_error_at = Some(now_epoch_s());
        } else {
            self.clock_persist_error = None;
            self.clock_persist_error_at = None;
        }
    }

    fn persist_raft_state_now(&self) -> Result<(), DbError> {
        let persisted =
            PersistedRaftState::capture(&self.replication.leader, &self.replication.membership);
        persist_raft_state(&self.wal_path, &persisted).map_err(|err| DbError::io(err.to_string()))
    }

    fn persist_raft_state_best_effort(&mut self) {
        if let Err(err) = self.persist_raft_state_now() {
            self.raft_persist_error = Some(err.message);
            self.raft_persist_error_at = Some(now_epoch_s());
        } else {
            self.raft_persist_error = None;
            self.raft_persist_error_at = None;
        }
    }

    fn persist_raft_state_required(&mut self) -> Result<(), DbError> {
        match self.persist_raft_state_now() {
            Ok(()) => {
                self.raft_persist_error = None;
                self.raft_persist_error_at = None;
                Ok(())
            }
            Err(err) => {
                self.raft_persist_error = Some(err.message.clone());
                self.raft_persist_error_at = Some(now_epoch_s());
                Err(err)
            }
        }
    }

    pub fn health_status(&self) -> DbHealthStatus {
        DbHealthStatus {
            clock_persist_error: self.clock_persist_error.clone(),
            clock_persist_error_at: self.clock_persist_error_at,
            raft_persist_error: self.raft_persist_error.clone(),
            raft_persist_error_at: self.raft_persist_error_at,
            cdc_checkpoint_persist_error: self.cdc_checkpoint_persist_error.clone(),
            cdc_checkpoint_persist_error_at: self.cdc_checkpoint_persist_error_at,
        }
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

    pub fn submit_batch(&mut self, batch: &[BatchOp]) -> Result<u64, DbError> {
        Self::validate_batch(batch)?;

        let frame = build_append_frame(batch);
        let required_term = self
            .replication
            .leader
            .current_term
            .max(self.raft_current_term);
        let required_index = self
            .replication
            .leader
            .last_log_index()
            .saturating_add(frame.command_count as u64);

        let mut shadow_versions: HashMap<Vec<u8>, Option<u64>> = HashMap::new();
        let mut staged_records = Vec::with_capacity(batch.len());
        let mut staged_ops = Vec::with_capacity(batch.len());
        let mut max_version = self.clock.peek().pack();

        for op in batch {
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
            let user_key = encode_user_key(namespace, key)?;
            let current = shadow_versions
                .get(&user_key)
                .copied()
                .unwrap_or_else(|| self.memtable.latest_version(&user_key));
            validate_expected_version(expected_version, current)?;

            let version = self.tick_clock();
            max_version = max_version.max(version);
            let cache_key = user_key.clone();
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
                        user_key,
                        cache_key,
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
                        value: Vec::new(),
                        version,
                    });
                    staged_ops.push(StagedApplyOp::Delete {
                        user_key,
                        cache_key,
                        namespace: namespace.clone(),
                        key: key.clone(),
                        version,
                    });
                }
            }
        }

        let staged_entries: Vec<LogEntry> = frame
            .commands
            .iter()
            .enumerate()
            .map(|(offset, command)| LogEntry {
                index: self
                    .replication
                    .leader
                    .last_log_index()
                    .saturating_add(offset as u64)
                    .saturating_add(1),
                term: required_term,
                payload: command_payload(command),
            })
            .collect();

        self.replicate_entries_for_quorum(required_term, required_index, &staged_entries)?;

        self.wal
            .append_batch(&staged_records)
            .map_err(|err| DbError::io(err.to_string()))?;
        self.apply_staged_ops(&staged_ops);

        self.replication.leader.commit_index = required_index;
        self.raft_last_log_index = self.replication.leader.last_log_index();
        self.raft_last_committed_index = self.replication.leader.commit_index;
        self.raft_current_term = self.replication.leader.current_term;
        self.persist_raft_state_best_effort();
        self.persist_clock_state_best_effort(max_version);
        Ok(max_version)
    }

    fn refresh_replication_followers(&mut self) {
        let mut membership_nodes = self.replication.membership.voters().clone();
        membership_nodes.extend(self.replication.membership.learners().iter().copied());
        if let Some(joint) = self.replication.membership.joint() {
            membership_nodes.extend(joint.outgoing_voters.iter().copied());
            membership_nodes.extend(joint.incoming_voters.iter().copied());
            membership_nodes.extend(joint.outgoing_learners.iter().copied());
        }

        self.replication
            .followers
            .retain(|node_id, _| *node_id != LOCAL_NODE_ID && membership_nodes.contains(node_id));
        for node_id in membership_nodes {
            if node_id == LOCAL_NODE_ID {
                continue;
            }
            self.replication
                .followers
                .entry(node_id)
                .or_insert_with(|| NodeState::with_timing(node_id, 0, 10));
        }
    }

    pub fn set_membership_voters(
        &mut self,
        voters: impl IntoIterator<Item = u64>,
    ) -> Result<(), DbError> {
        let previous_membership = self.replication.membership.clone();
        let previous_followers = self.replication.followers.clone();
        self.replication.membership = MembershipConfig::new(voters)
            .map_err(|err| DbError::invalid_argument(format!("membership invalid: {err:?}")))?;
        self.refresh_replication_followers();
        if let Err(err) = self.persist_raft_state_required() {
            self.replication.membership = previous_membership;
            self.replication.followers = previous_followers;
            return Err(err);
        }
        Ok(())
    }

    pub fn begin_membership_change(
        &mut self,
        change: MembershipChange,
        log_index: u64,
    ) -> Result<(), DbError> {
        let previous_membership = self.replication.membership.clone();
        let previous_followers = self.replication.followers.clone();
        self.replication
            .membership
            .begin_joint_change(change, log_index)
            .map_err(|err| {
                DbError::invalid_argument(format!("membership change rejected: {err:?}"))
            })?;
        self.refresh_replication_followers();
        if let Err(err) = self.persist_raft_state_required() {
            self.replication.membership = previous_membership;
            self.replication.followers = previous_followers;
            return Err(err);
        }
        Ok(())
    }

    pub fn commit_membership_change(&mut self) -> Result<(), DbError> {
        let previous_membership = self.replication.membership.clone();
        let previous_followers = self.replication.followers.clone();
        self.replication
            .membership
            .commit_joint_change()
            .map_err(|err| {
                DbError::invalid_argument(format!("membership commit rejected: {err:?}"))
            })?;
        self.refresh_replication_followers();
        if let Err(err) = self.persist_raft_state_required() {
            self.replication.membership = previous_membership;
            self.replication.followers = previous_followers;
            return Err(err);
        }
        Ok(())
    }

    pub fn abort_membership_change(&mut self) -> Result<(), DbError> {
        let previous_membership = self.replication.membership.clone();
        let previous_followers = self.replication.followers.clone();
        self.replication
            .membership
            .abort_joint_change()
            .map_err(|err| {
                DbError::invalid_argument(format!("membership abort rejected: {err:?}"))
            })?;
        self.refresh_replication_followers();
        if let Err(err) = self.persist_raft_state_required() {
            self.replication.membership = previous_membership;
            self.replication.followers = previous_followers;
            return Err(err);
        }
        Ok(())
    }

    fn replicate_entries_for_quorum(
        &mut self,
        required_term: u64,
        required_index: u64,
        entries: &[LogEntry],
    ) -> Result<(), DbError> {
        let mut leader = self.replication.leader.clone();
        for entry in entries {
            leader
                .append_log_entry_checked(entry.clone())
                .map_err(|_| DbError::invalid_argument("non-contiguous leader log append"))?;
        }

        let mut followers = self.replication.followers.clone();
        let mut voter_ids = self.replication.membership.voters().clone();
        if let Some(joint) = self.replication.membership.joint() {
            voter_ids.extend(joint.outgoing_voters.iter().copied());
            voter_ids.extend(joint.incoming_voters.iter().copied());
        }
        let mut follower_responses = Vec::new();
        let mut replication_error: Option<DbError> = None;
        for node_id in voter_ids {
            if node_id == LOCAL_NODE_ID {
                continue;
            }
            let follower_state = followers
                .entry(node_id)
                .or_insert_with(|| NodeState::with_timing(node_id, 0, 10));
            match replicate_to_follower(&leader, follower_state, leader.commit_index) {
                Ok(response) => {
                    follower_responses.push(FollowerAppendResponse {
                        node_id,
                        response,
                        replication_latency_ns: 0,
                        fsync_latency_ns: 0,
                    });
                }
                Err(err) => {
                    if replication_error.is_none() {
                        replication_error = Some(err);
                    }
                }
            }
        }

        #[cfg(test)]
        if !self.pending_append_responses.is_empty() {
            follower_responses = self.pending_append_responses.clone();
        }

        let ack_decision = evaluate_leader_ack(&LeaderAckInput {
            voters: self.replication.membership.voters().len(),
            leader_durable: true,
            required_term,
            required_index,
            follower_responses: follower_responses.clone(),
        });
        let mut durable_acks = BTreeSet::from([LOCAL_NODE_ID]);
        for follower in &follower_responses {
            if response_is_durable_ack(&follower.response, required_term, required_index) {
                durable_acks.insert(follower.node_id);
            }
        }
        if !ack_decision.ack_emitted
            || !self
                .replication
                .membership
                .has_durable_quorum(&durable_acks)
        {
            if let Some(err) = replication_error {
                return Err(err);
            }
            return Err(DbError::limit(format!(
                "durability quorum not reached; durable_acks={} quorum={}",
                ack_decision.durable_acks, ack_decision.quorum_size
            )));
        }

        self.replication.leader = leader;
        self.replication.followers = followers;
        #[cfg(test)]
        self.pending_append_responses.clear();
        Ok(())
    }

    fn apply_staged_ops(&mut self, staged_ops: &[StagedApplyOp]) {
        let mut max_applied_version = None;
        for op in staged_ops {
            match op {
                StagedApplyOp::Put {
                    user_key,
                    cache_key,
                    namespace,
                    key,
                    value,
                    version,
                } => {
                    self.memtable
                        .apply(user_key.clone(), *version, Some(value.clone()));
                    self.cdc
                        .emit_put(namespace.clone(), key.clone(), value.clone(), *version);
                    self.read_path.observe_present_key(cache_key);
                    self.safe_time.observe_shard_safe_time(
                        String::from_utf8_lossy(namespace).to_string(),
                        LOCAL_REGION_ID,
                        *version,
                    );
                    max_applied_version = Some(max_applied_version.unwrap_or(0).max(*version));
                }
                StagedApplyOp::Delete {
                    user_key,
                    cache_key,
                    namespace,
                    key,
                    version,
                } => {
                    self.memtable.apply(user_key.clone(), *version, None);
                    self.cdc
                        .emit_delete(namespace.clone(), key.clone(), *version);
                    self.read_path.observe_absent_key(cache_key);
                    self.safe_time.observe_shard_safe_time(
                        String::from_utf8_lossy(namespace).to_string(),
                        LOCAL_REGION_ID,
                        *version,
                    );
                    max_applied_version = Some(max_applied_version.unwrap_or(0).max(*version));
                }
            }
        }
        if let Some(version) = max_applied_version {
            self.safe_time
                .observe_shard_safe_time("clock", LOCAL_REGION_ID, version);
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
        self.read_path.read_point(&user_key, || {
            self.memtable
                .visible(&user_key, read_version)
                .map(|v| v.to_vec())
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
        let rows = self
            .memtable
            .range_visible(&start, &end, read_version, limit);
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
            rows.push((user_key, row.1, row.2));
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

    pub fn snapshot_start(&mut self) -> Result<u64, DbError> {
        let snapshot_id = self.next_snapshot_id;
        self.next_snapshot_id = self.next_snapshot_id.saturating_add(1);
        let created_ts = self.tick_clock();
        self.snapshots.insert(
            snapshot_id,
            SnapshotRecord {
                created_ts,
                progress: 100,
                restored_ts: None,
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
        let created_ts = self
            .snapshots
            .get(&snapshot_id)
            .map(|r| r.created_ts)
            .ok_or_else(|| DbError::invalid_argument("unknown snapshot id"))?;
        let restored_ts = self.tick_clock().max(created_ts);
        let record = self
            .snapshots
            .get_mut(&snapshot_id)
            .ok_or_else(|| DbError::invalid_argument("unknown snapshot id"))?;
        record.restored_ts = Some(restored_ts);
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

    pub fn flush_clock_state(&self) -> Result<(), DbError> {
        self.persist_clock_state(self.clock.peek().pack())
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

    #[cfg(test)]
    fn inject_cdc_checkpoint_persist_failure(&mut self) {
        self.fail_next_cdc_checkpoint_persist = true;
    }
}

struct DbRegistry {
    next_handle: AtomicI64,
    handles: Mutex<HashMap<i64, Arc<Mutex<DbEngine>>>>,
}

impl DbRegistry {
    fn new() -> Self {
        Self {
            next_handle: AtomicI64::new(1),
            handles: Mutex::new(HashMap::new()),
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
        let entries = leader
            .log
            .iter()
            .filter(|entry| entry.index >= next_index)
            .cloned()
            .collect::<Vec<_>>();
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

fn db_for_handle(handle: i64) -> Result<Arc<Mutex<DbEngine>>, DbError> {
    lock_registry_handles()?
        .get(&handle)
        .cloned()
        .ok_or_else(|| DbError::invalid_argument("unknown DB handle"))
}

fn lock_registry_handles()
-> Result<MutexGuard<'static, HashMap<i64, Arc<Mutex<DbEngine>>>>, DbError> {
    registry()
        .handles
        .lock()
        .map_err(|_| DbError::io("DB registry lock poisoned"))
}

fn lock_engine(db: &Arc<Mutex<DbEngine>>) -> Result<MutexGuard<'_, DbEngine>, DbError> {
    db.lock()
        .map_err(|_| DbError::io("DB engine lock poisoned"))
}

fn with_engine<T, F>(handle: i64, f: F) -> Result<T, DbError>
where
    F: FnOnce(&DbEngine) -> Result<T, DbError>,
{
    let db = db_for_handle(handle)?;
    let engine = lock_engine(&db)?;
    f(&engine)
}

fn with_engine_mut<T, F>(handle: i64, f: F) -> Result<T, DbError>
where
    F: FnOnce(&mut DbEngine) -> Result<T, DbError>,
{
    let db = db_for_handle(handle)?;
    let mut engine = lock_engine(&db)?;
    f(&mut engine)
}

pub fn open_db(data_dir: &Path) -> Result<i64, DbError> {
    std::fs::create_dir_all(data_dir).map_err(|err| DbError::io(err.to_string()))?;
    let wal_path = wal_path_from(data_dir);
    let engine = DbEngine::open(&wal_path)?;
    let handle = registry().next_handle.fetch_add(1, Ordering::Relaxed);
    lock_registry_handles()?.insert(handle, Arc::new(Mutex::new(engine)));
    Ok(handle)
}

pub fn close_db(handle: i64) -> bool {
    let flush_ok = match db_for_handle(handle) {
        Ok(db) => match lock_engine(&db) {
            Ok(engine) => engine.flush_clock_state().is_ok(),
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
    with_engine_mut(handle, |engine| {
        engine.submit_batch(&[BatchOp::Put {
            namespace,
            key,
            value,
            expected_version,
        }])
    })
}

pub fn submit_batch(handle: i64, batch: &[BatchOp]) -> Result<u64, DbError> {
    with_engine_mut(handle, |engine| engine.submit_batch(batch))
}

pub fn read_point(
    handle: i64,
    namespace: Vec<u8>,
    key: Vec<u8>,
) -> Result<Option<Vec<u8>>, DbError> {
    read_point_consistent(handle, namespace, key, ReadConsistency::Strong, None)
}

pub fn read_point_consistent(
    handle: i64,
    namespace: Vec<u8>,
    key: Vec<u8>,
    consistency: ReadConsistency,
    requested_ts: Option<u64>,
) -> Result<Option<Vec<u8>>, DbError> {
    with_engine(handle, |engine| {
        engine.read_point(&namespace, &key, consistency, requested_ts)
    })
}

pub fn read_range(
    handle: i64,
    namespace: Vec<u8>,
    start_key: Vec<u8>,
    end_key: Vec<u8>,
    limit: usize,
) -> Result<Vec<(Vec<u8>, Vec<u8>, u64)>, DbError> {
    read_range_consistent(
        handle,
        namespace,
        start_key,
        end_key,
        limit,
        ReadConsistency::Strong,
        None,
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
    with_engine(handle, |engine| {
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
    with_engine_mut(handle, |engine| engine.txn_commit(txn_id))
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
    with_engine_mut(handle, |engine| engine.restore_snapshot(snapshot_id))
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
    with_engine(handle, |engine| Ok(engine.health_status()))
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
    use crate::db::types::ErrorCode;
    use crate::db::wal::format::{Record, RecordKind, encode};
    use crate::db::writer::DetachedWriterQueue;
    use std::collections::BTreeSet;
    use std::fs::OpenOptions;
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::sync::atomic::{AtomicU64, Ordering};

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
        let engine = db.lock().expect("DB engine lock");
        let stats = engine.read_stats();
        assert_eq!(stats.point_cache_misses, 1);
        assert_eq!(stats.point_cache_hits, 1);
        assert_eq!(stats.negative_shortcuts, 0);
        drop(engine);
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
            let engine = db.lock().expect("DB engine lock");
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
            namespace: b"core".to_vec(),
            key: b"partial".to_vec(),
            value: b"bad".to_vec(),
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
            namespace: b"core".to_vec(),
            key: b"k1".to_vec(),
            value: b"v1".to_vec(),
            expected_version: None,
        })
        .expect("first enqueue");
        let err = q
            .push(BatchOp::Put {
                namespace: b"core".to_vec(),
                key: b"k2".to_vec(),
                value: b"v2".to_vec(),
                expected_version: None,
            })
            .expect_err("queue should backpressure");
        assert_eq!(err.code, ErrorCode::LimitExceeded);
        assert!(err.message.contains("RETRY_AFTER_MS=25"));
    }

    #[test]
    fn cdc_emits_committed_apply_order_with_stable_commit_sequence() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        submit_batch(
            handle,
            &[
                BatchOp::Put {
                    namespace: b"core".to_vec(),
                    key: b"k1".to_vec(),
                    value: b"v1".to_vec(),
                    expected_version: None,
                },
                BatchOp::Delete {
                    namespace: b"core".to_vec(),
                    key: b"k1".to_vec(),
                    expected_version: None,
                },
            ],
        )
        .expect("submit batch");

        let db = db_for_handle(handle).expect("db handle");
        let engine = db.lock().expect("DB engine lock");
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
        let engine = db.lock().expect("DB engine lock");
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
                    namespace: b"core".to_vec(),
                    key: b"k1".to_vec(),
                    value: b"v1".to_vec(),
                    expected_version: None,
                },
                BatchOp::Put {
                    namespace: b"core".to_vec(),
                    key: b"k2".to_vec(),
                    value: b"v2".to_vec(),
                    expected_version: None,
                },
                BatchOp::Delete {
                    namespace: b"core".to_vec(),
                    key: b"k1".to_vec(),
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
                    namespace: b"core".to_vec(),
                    key: b"k1".to_vec(),
                    value: b"v1".to_vec(),
                    expected_version: None,
                },
                BatchOp::Put {
                    namespace: b"aux".to_vec(),
                    key: b"k9".to_vec(),
                    value: b"v9".to_vec(),
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
                    namespace: b"core".to_vec(),
                    key: b"k1".to_vec(),
                    value: b"v1".to_vec(),
                    expected_version: None,
                },
                BatchOp::Put {
                    namespace: b"core".to_vec(),
                    key: b"k2".to_vec(),
                    value: b"v2".to_vec(),
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
                    namespace: b"core".to_vec(),
                    key: b"k1".to_vec(),
                    value: b"v1".to_vec(),
                    expected_version: None,
                },
                BatchOp::Put {
                    namespace: b"core".to_vec(),
                    key: b"k2".to_vec(),
                    value: b"v2".to_vec(),
                    expected_version: None,
                },
                BatchOp::Put {
                    namespace: b"core".to_vec(),
                    key: b"k3".to_vec(),
                    value: b"v3".to_vec(),
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
                namespace: b"core".to_vec(),
                key: b"k1".to_vec(),
                value: b"v1".to_vec(),
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
                namespace: b"core".to_vec(),
                key: b"k1".to_vec(),
                value: b"v1".to_vec(),
                expected_version: None,
            }],
        )
        .expect("submit batch");
        assert_eq!(cdc_ack(handle, "orders".to_string(), 1).expect("ack"), 1);

        {
            let db = db_for_handle(handle).expect("db handle");
            let mut engine = db.lock().expect("DB engine lock");
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
    fn submit_batch_wal_mid_batch_failure_is_atomic() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        {
            let db = db_for_handle(handle).expect("db handle");
            let engine = db.lock().expect("DB engine lock");
            engine.wal.fail_batch_after_records(1);
        }

        let err = submit_batch(
            handle,
            &[
                BatchOp::Put {
                    namespace: b"core".to_vec(),
                    key: b"k1".to_vec(),
                    value: b"v1".to_vec(),
                    expected_version: None,
                },
                BatchOp::Put {
                    namespace: b"core".to_vec(),
                    key: b"k2".to_vec(),
                    value: b"v2".to_vec(),
                    expected_version: None,
                },
            ],
        )
        .expect_err("mid-batch WAL failure must fail batch");
        assert_eq!(err.code, ErrorCode::Io);

        let k1 = read_point(handle, b"core".to_vec(), b"k1".to_vec()).expect("read k1");
        let k2 = read_point(handle, b"core".to_vec(), b"k2".to_vec()).expect("read k2");
        assert_eq!(k1, None);
        assert_eq!(k2, None);
        assert!(close_db(handle));
    }

    #[test]
    fn submit_batch_wal_sync_failure_is_atomic() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        {
            let db = db_for_handle(handle).expect("db handle");
            let engine = db.lock().expect("DB engine lock");
            engine.wal.fail_next_sync();
        }

        let err = submit_batch(
            handle,
            &[BatchOp::Put {
                namespace: b"core".to_vec(),
                key: b"k1".to_vec(),
                value: b"v1".to_vec(),
                expected_version: None,
            }],
        )
        .expect_err("WAL sync failure must fail batch");
        assert_eq!(err.code, ErrorCode::Io);
        let k1 = read_point(handle, b"core".to_vec(), b"k1".to_vec()).expect("read k1");
        assert_eq!(k1, None);
        assert!(close_db(handle));
    }

    #[test]
    fn submit_batch_wal_prewrite_failure_is_atomic() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        {
            let db = db_for_handle(handle).expect("db handle");
            let engine = db.lock().expect("DB engine lock");
            engine.wal.fail_next_batch_write();
        }

        let err = submit_batch(
            handle,
            &[BatchOp::Put {
                namespace: b"core".to_vec(),
                key: b"k1".to_vec(),
                value: b"v1".to_vec(),
                expected_version: None,
            }],
        )
        .expect_err("WAL write failure must fail batch");
        assert_eq!(err.code, ErrorCode::Io);
        let k1 = read_point(handle, b"core".to_vec(), b"k1".to_vec()).expect("read k1");
        assert_eq!(k1, None);
        {
            let db = db_for_handle(handle).expect("db handle");
            let engine = db.lock().expect("DB engine lock");
            engine.wal.clear_failpoints();
        }
        assert!(close_db(handle));
    }

    #[test]
    fn submit_batch_enforces_quorum_gate_for_multi_voter_mode() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let db = db_for_handle(handle).expect("db handle");
        {
            let mut engine = db.lock().expect("DB engine lock");
            engine.set_membership_voters([1, 2, 3]).expect("set voters");
            let stalled_term = engine.replication.leader.current_term.saturating_add(5);
            for follower in engine.replication.followers.values_mut() {
                follower.current_term = stalled_term;
            }
        }

        let err = submit_batch(
            handle,
            &[BatchOp::Put {
                namespace: b"core".to_vec(),
                key: b"k".to_vec(),
                value: b"v".to_vec(),
                expected_version: None,
            }],
        )
        .expect_err("must fail without quorum responses");
        assert_eq!(err.code, ErrorCode::LimitExceeded);
        assert!(err.message.contains("durability quorum not reached"));
        let value = read_point(handle, b"core".to_vec(), b"k".to_vec()).expect("read after fail");
        assert!(value.is_none(), "quorum failure must not apply writes");
        assert!(close_db(handle));
    }

    #[test]
    fn submit_batch_accepts_quorum_with_append_responses() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let db = db_for_handle(handle).expect("db handle");
        {
            let mut engine = db.lock().expect("DB engine lock");
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
                namespace: b"core".to_vec(),
                key: b"k".to_vec(),
                value: b"v".to_vec(),
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
            let mut engine = db.lock().expect("DB engine lock");
            engine.set_membership_voters([1, 2, 3]).expect("set voters");
            let stalled_term = engine.replication.leader.current_term.saturating_add(5);
            for follower in engine.replication.followers.values_mut() {
                follower.current_term = stalled_term;
            }
        }

        let rejected = submit_batch(
            handle,
            &[BatchOp::Put {
                namespace: b"core".to_vec(),
                key: b"k1".to_vec(),
                value: b"v1".to_vec(),
                expected_version: None,
            }],
        );
        assert!(
            rejected.is_err(),
            "first batch must be rejected without quorum"
        );

        {
            let mut engine = db.lock().expect("DB engine lock");
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
                namespace: b"core".to_vec(),
                key: b"k2".to_vec(),
                value: b"v2".to_vec(),
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
            let mut engine = db.lock().expect("DB engine lock");
            engine.set_membership_voters([1, 2, 3]).expect("set voters");
            engine.raft_current_term = 5;
            engine.pending_append_responses = vec![FollowerAppendResponse {
                node_id: 2,
                response: AppendEntriesResponse {
                    term: 4,
                    success: true,
                    match_index: u64::MAX,
                    conflict_index: None,
                },
                replication_latency_ns: 10,
                fsync_latency_ns: 5,
            }];
        }
        let err = submit_batch(
            handle,
            &[BatchOp::Put {
                namespace: b"core".to_vec(),
                key: b"k".to_vec(),
                value: b"v".to_vec(),
                expected_version: None,
            }],
        )
        .expect_err("stale-term response must not count toward quorum");
        assert_eq!(err.code, ErrorCode::LimitExceeded);
        assert!(close_db(handle));
    }

    #[test]
    fn joint_membership_requires_dual_quorum_in_live_write_path() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let db = db_for_handle(handle).expect("db handle");
        {
            let mut engine = db.lock().expect("DB engine lock");
            engine.set_membership_voters([1, 2, 3]).expect("set voters");
            engine
                .begin_membership_change(MembershipChange::AddVoter { node_id: 4 }, 7)
                .expect("begin joint");
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

        let err = submit_batch(
            handle,
            &[BatchOp::Put {
                namespace: b"core".to_vec(),
                key: b"joint-k".to_vec(),
                value: b"v1".to_vec(),
                expected_version: None,
            }],
        )
        .expect_err("single old-quorum ack must fail dual quorum");
        assert_eq!(err.code, ErrorCode::LimitExceeded);

        {
            let mut engine = db.lock().expect("DB engine lock");
            engine.pending_append_responses = vec![
                FollowerAppendResponse {
                    node_id: 2,
                    response: AppendEntriesResponse {
                        term: engine.raft_current_term,
                        success: true,
                        match_index: u64::MAX,
                        conflict_index: None,
                    },
                    replication_latency_ns: 10,
                    fsync_latency_ns: 5,
                },
                FollowerAppendResponse {
                    node_id: 4,
                    response: AppendEntriesResponse {
                        term: engine.raft_current_term,
                        success: true,
                        match_index: u64::MAX,
                        conflict_index: None,
                    },
                    replication_latency_ns: 10,
                    fsync_latency_ns: 5,
                },
            ];
        }

        submit_batch(
            handle,
            &[BatchOp::Put {
                namespace: b"core".to_vec(),
                key: b"joint-k".to_vec(),
                value: b"v2".to_vec(),
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
            let mut engine = db.lock().expect("DB engine lock");
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
                namespace: b"core".to_vec(),
                key: b"abort-joint-k".to_vec(),
                value: b"v".to_vec(),
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
        let engine = db.lock().expect("DB engine lock");
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
            let mut engine = db.lock().expect("DB engine lock");
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
            let mut engine = db.lock().expect("DB engine lock");
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
            let mut engine = db.lock().expect("DB engine lock");
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
            let mut engine = db.lock().expect("DB engine lock");
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
            let mut engine = db.lock().expect("DB engine lock");
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
            let mut engine = db.lock().expect("DB engine lock");
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
            let mut engine = db.lock().expect("DB engine lock");
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
            let mut engine = db.lock().expect("DB engine lock");
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
            let mut engine = db.lock().expect("DB engine lock");
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
            let mut engine = db.lock().expect("DB engine lock");
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
            let mut engine = db.lock().expect("DB engine lock");
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
            let mut engine = db.lock().expect("DB engine lock");
            let txn = engine.txn_begin().expect("txn begin");
            engine
                .txn_lock_key(txn, b"core", b"restart-k")
                .expect("restart should not retain stale intents");
        }
        assert!(close_db(reopened));
    }

    #[test]
    fn snapshot_lifecycle_returns_progress_and_validates_ids() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let snapshot_id = snapshot_start(handle).expect("start snapshot");
        let progress = snapshot_status(handle, snapshot_id).expect("status");
        assert_eq!(progress, 100);
        restore_snapshot(handle, snapshot_id).expect("restore snapshot");

        let missing = snapshot_status(handle, snapshot_id.saturating_add(99))
            .expect_err("unknown snapshot id");
        assert_eq!(missing.code, ErrorCode::InvalidArgument);
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
            let engine = db.lock().expect("DB engine lock");
            engine.clock_packed()
        };
        assert!(close_db(handle));

        let reopened = open_db(&dir).expect("reopen");
        let after_reopen = {
            let db = db_for_handle(reopened).expect("db handle");
            let engine = db.lock().expect("DB engine lock");
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
        let handle = open_db(&dir).expect("open db");
        membership_set_voters(handle, vec![1, 2, 3]).expect("set voters");
        membership_begin_joint_change(handle, MembershipChange::AddVoter { node_id: 4 }, 17)
            .expect("begin joint");
        assert!(close_db(handle));

        let reopened = open_db(&dir).expect("reopen");
        let db = db_for_handle(reopened).expect("db handle");
        let engine = db.lock().expect("DB engine lock");
        assert_eq!(
            engine.replication.membership.voters(),
            &BTreeSet::from([1, 2, 3])
        );
        let joint = engine
            .replication
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
        let engine = db.lock().expect("DB engine lock");
        let leader = &engine.replication.leader;
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
            let _guard = poison_target.lock().expect("lock");
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
            let mut engine = db.lock().expect("DB engine lock");
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
        let engine = db.lock().expect("DB engine lock");
        let window = engine.uncertainty_window();
        assert!(window.upper_bound >= window.lower_bound);
        drop(engine);
        assert!(close_db(handle));
    }
}
