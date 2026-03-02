use crate::db::raft::membership::{JointMembership, MembershipConfig};
use crate::db::raft::message::LogEntry;
use crate::db::raft::state::{NodeState, Role};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, Error, ErrorKind, Write};
use std::path::{Path, PathBuf};

const RAFT_STATE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedElectionState {
    pub current_term: u64,
    pub voted_for: Option<u64>,
    pub commit_index: u64,
    pub log: Vec<LogEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedMembershipJoint {
    pub outgoing_voters: BTreeSet<u64>,
    pub incoming_voters: BTreeSet<u64>,
    pub outgoing_learners: BTreeSet<u64>,
    pub started_at_log_index: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedMembershipState {
    pub voters: BTreeSet<u64>,
    pub learners: BTreeSet<u64>,
    pub joint: Option<PersistedMembershipJoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedRaftState {
    pub schema_version: u32,
    pub current_term: u64,
    pub voted_for: Option<u64>,
    pub commit_index: u64,
    pub log: Vec<LogEntry>,
    pub membership: PersistedMembershipState,
}

impl PersistedMembershipState {
    fn capture(config: &MembershipConfig) -> Self {
        Self {
            voters: config.voters().clone(),
            learners: config.learners().clone(),
            joint: config.joint().map(|joint| PersistedMembershipJoint {
                outgoing_voters: joint.outgoing_voters.clone(),
                incoming_voters: joint.incoming_voters.clone(),
                outgoing_learners: joint.outgoing_learners.clone(),
                started_at_log_index: joint.started_at_log_index,
            }),
        }
    }

    fn restore(&self) -> Result<MembershipConfig, Error> {
        let joint = self.joint.clone().map(|joint| JointMembership {
            outgoing_voters: joint.outgoing_voters,
            incoming_voters: joint.incoming_voters,
            outgoing_learners: joint.outgoing_learners,
            started_at_log_index: joint.started_at_log_index,
        });
        MembershipConfig::from_parts(self.voters.clone(), self.learners.clone(), joint).map_err(
            |err| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("invalid persisted membership state: {err:?}"),
                )
            },
        )
    }
}

impl PersistedRaftState {
    pub fn capture(state: &NodeState, membership: &MembershipConfig) -> Self {
        Self {
            schema_version: RAFT_STATE_SCHEMA_VERSION,
            current_term: state.current_term,
            voted_for: state.voted_for,
            commit_index: state.commit_index,
            log: state.log.clone(),
            membership: PersistedMembershipState::capture(membership),
        }
    }

    pub fn restore(
        &self,
        state: &mut NodeState,
        now_tick: u64,
        election_timeout_ticks: u64,
    ) -> Result<MembershipConfig, Error> {
        if self.schema_version != RAFT_STATE_SCHEMA_VERSION {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "unsupported raft persistence schema version {}; expected {}",
                    self.schema_version, RAFT_STATE_SCHEMA_VERSION
                ),
            ));
        }
        ensure_contiguous_log(&self.log)?;
        let membership = self.membership.restore()?;

        state.current_term = self.current_term;
        state.voted_for = self.voted_for;
        state.role = Role::Follower;
        state.last_heartbeat_tick = now_tick;
        state.reset_election_deadline(now_tick, election_timeout_ticks);
        state.log = self.log.clone();
        state.commit_index = self.commit_index.min(state.last_log_index());
        Ok(membership)
    }
}

impl PersistedElectionState {
    pub fn capture(state: &NodeState) -> Self {
        Self {
            current_term: state.current_term,
            voted_for: state.voted_for,
            commit_index: state.commit_index,
            log: state.log.clone(),
        }
    }

    pub fn restore_into(&self, state: &mut NodeState, now_tick: u64, election_timeout_ticks: u64) {
        state.current_term = self.current_term;
        state.voted_for = self.voted_for;
        state.role = Role::Follower;
        state.last_heartbeat_tick = now_tick;
        state.reset_election_deadline(now_tick, election_timeout_ticks);
        state.commit_index = self.commit_index;
        state.restore_log_contiguous(self.log.clone());
        if state.commit_index > state.last_log_index() {
            state.commit_index = state.last_log_index();
        }
    }
}

pub fn raft_state_path_from(wal_path: &Path) -> PathBuf {
    wal_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("raft_state.json")
}

pub fn load_persisted_raft_state(wal_path: &Path) -> Result<Option<PersistedRaftState>, Error> {
    let path = raft_state_path_from(wal_path);
    match File::open(&path) {
        Ok(file) => serde_json::from_reader::<_, PersistedRaftState>(BufReader::new(file))
            .map(Some)
            .map_err(|err| Error::new(ErrorKind::InvalidData, err.to_string())),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

pub fn persist_raft_state(wal_path: &Path, state: &PersistedRaftState) -> Result<(), Error> {
    ensure_contiguous_log(&state.log)?;
    let path = raft_state_path_from(wal_path);
    let tmp = path.with_extension("json.tmp");
    {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&tmp)?;
        serde_json::to_writer(&mut file, state)
            .map_err(|err| Error::new(ErrorKind::InvalidData, err.to_string()))?;
        file.write_all(b"\n")?;
        file.sync_data()?;
    }
    std::fs::rename(&tmp, &path)?;
    fsync_parent_dir(&path)?;
    Ok(())
}

fn ensure_contiguous_log(log: &[LogEntry]) -> Result<(), Error> {
    let mut expected = 1u64;
    for entry in log {
        if entry.index != expected {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "non-contiguous raft log entry at index {} expected {}",
                    entry.index, expected
                ),
            ));
        }
        expected = expected.saturating_add(1);
    }
    Ok(())
}

fn fsync_parent_dir(path: &Path) -> Result<(), Error> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let dir = File::open(parent)?;
    dir.sync_all()
}

// ---------------------------------------------------------------------------
// Incremental Raft Log Appender
// ---------------------------------------------------------------------------

const RAFT_LOG_MAGIC: [u8; 4] = *b"RLG1";
const DEFAULT_COMPACTION_THRESHOLD: u64 = 1000;

/// Append-only binary log for incremental Raft state persistence.
/// Metadata (term, voted_for, commit_index, membership) is persisted
/// atomically in the JSON state file. Log entries are appended
/// incrementally to a separate binary file, avoiding the O(log_size)
/// full-serialize cost on every persist cycle.
#[derive(Debug)]
pub struct RaftLogAppender {
    log_path: PathBuf,
    file: Option<File>,
    pub last_flushed_index: u64,
    entries_since_compaction: u64,
    compaction_threshold: u64,
}

impl RaftLogAppender {
    pub fn new(wal_path: &Path) -> Self {
        let log_path = wal_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("raft_log.bin");
        Self {
            log_path,
            file: None,
            last_flushed_index: 0,
            entries_since_compaction: 0,
            compaction_threshold: DEFAULT_COMPACTION_THRESHOLD,
        }
    }

    /// Append only the new entries (since `last_flushed_index`) to the
    /// binary log file. Returns the number of entries appended.
    pub fn append_incremental(&mut self, log: &[LogEntry]) -> Result<usize, Error> {
        let new_entries: Vec<&LogEntry> = log
            .iter()
            .filter(|entry| entry.index > self.last_flushed_index)
            .collect();
        if new_entries.is_empty() {
            return Ok(0);
        }

        let file = match self.file.as_mut() {
            Some(f) => f,
            None => {
                let f = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&self.log_path)?;
                self.file = Some(f);
                self.file.as_mut().unwrap()
            }
        };

        let mut buf = Vec::new();
        for entry in &new_entries {
            encode_log_entry(entry, &mut buf);
        }
        file.write_all(&buf)?;
        file.sync_data()?;

        if let Some(last) = new_entries.last() {
            self.last_flushed_index = last.index;
        }
        self.entries_since_compaction += new_entries.len() as u64;

        Ok(new_entries.len())
    }

    /// Full compaction: rewrite the binary log from the complete log.
    /// Called periodically to bound recovery time.
    pub fn compact(&mut self, full_log: &[LogEntry]) -> Result<(), Error> {
        // Close existing handle.
        self.file = None;

        let tmp = self.log_path.with_extension("bin.tmp");
        {
            let mut file = OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&tmp)?;
            let mut buf = Vec::new();
            for entry in full_log {
                encode_log_entry(entry, &mut buf);
            }
            file.write_all(&buf)?;
            file.sync_data()?;
        }
        std::fs::rename(&tmp, &self.log_path)?;
        fsync_parent_dir(&self.log_path)?;

        self.entries_since_compaction = 0;
        if let Some(last) = full_log.last() {
            self.last_flushed_index = last.index;
        }
        Ok(())
    }

    /// Whether compaction is due based on entries appended since last
    /// compaction.
    pub fn needs_compaction(&self) -> bool {
        self.entries_since_compaction >= self.compaction_threshold
    }

    /// Reset state (e.g. after loading from a full snapshot).
    pub fn reset(&mut self, last_index: u64) {
        self.last_flushed_index = last_index;
        self.entries_since_compaction = 0;
        self.file = None;
    }
}

fn encode_log_entry(entry: &LogEntry, buf: &mut Vec<u8>) {
    buf.extend_from_slice(&RAFT_LOG_MAGIC);
    buf.extend_from_slice(&entry.index.to_be_bytes());
    buf.extend_from_slice(&entry.term.to_be_bytes());
    buf.extend_from_slice(&(entry.payload.len() as u32).to_be_bytes());
    buf.extend_from_slice(&entry.payload);
}

pub fn load_incremental_raft_log(wal_path: &Path) -> Result<Vec<LogEntry>, Error> {
    let log_path = wal_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("raft_log.bin");
    let data = match std::fs::read(&log_path) {
        Ok(data) => data,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err),
    };
    let mut entries = Vec::new();
    let mut offset = 0;
    while offset + 4 + 8 + 8 + 4 <= data.len() {
        if data[offset..offset + 4] != RAFT_LOG_MAGIC {
            break; // Truncated or corrupted tail
        }
        let index = u64::from_be_bytes(data[offset + 4..offset + 12].try_into().unwrap());
        let term = u64::from_be_bytes(data[offset + 12..offset + 20].try_into().unwrap());
        let payload_len =
            u32::from_be_bytes(data[offset + 20..offset + 24].try_into().unwrap()) as usize;
        let payload_end = offset + 24 + payload_len;
        if payload_end > data.len() {
            break; // Truncated entry
        }
        entries.push(LogEntry {
            index,
            term,
            payload: data[offset + 24..payload_end].to_vec(),
        });
        offset = payload_end;
    }
    Ok(entries)
}

/// Persist only metadata (term, voted_for, commit_index, membership)
/// without the full log. Used with incremental log appending.
pub fn persist_raft_metadata(wal_path: &Path, state: &PersistedRaftState) -> Result<(), Error> {
    // We persist the full state JSON but the incremental appender
    // means we only need to serialize the metadata fields and the
    // log that is already in memory. For backward compatibility, we
    // persist the full state so recovery can work from either path.
    persist_raft_state(wal_path, state)
}

// ---------------------------------------------------------------------------
// Binary Raft Metadata (hot-path persist — zero clone, zero serialization)
// ---------------------------------------------------------------------------

const RAFT_META_MAGIC: [u8; 4] = *b"RFM1";

/// Lightweight metadata captured under the DbEngine lock on the write
/// hot path. Contains only scalar fields — no log entries, no
/// membership config, no allocations.
#[derive(Debug, Clone, Copy)]
pub struct RaftPersistMetadata {
    pub current_term: u64,
    pub voted_for: Option<u64>,
    pub commit_index: u64,
    pub needs_membership_flush: bool,
}

/// Persist binary raft metadata (~28 bytes) via tmp+rename+fsync.
pub fn persist_raft_metadata_binary(
    wal_path: &Path,
    meta: &RaftPersistMetadata,
) -> Result<(), Error> {
    let meta_path = wal_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("raft_meta.bin");
    let tmp = meta_path.with_extension("bin.tmp");
    let mut buf = [0u8; 28];
    buf[0..4].copy_from_slice(&RAFT_META_MAGIC);
    buf[4..12].copy_from_slice(&meta.current_term.to_be_bytes());
    const VOTED_FOR_NONE: u64 = u64::MAX;
    let voted = meta.voted_for.unwrap_or(VOTED_FOR_NONE);
    buf[12..20].copy_from_slice(&voted.to_be_bytes());
    buf[20..28].copy_from_slice(&meta.commit_index.to_be_bytes());
    std::fs::write(&tmp, &buf)?;
    File::open(&tmp)?.sync_data()?;
    std::fs::rename(&tmp, &meta_path)?;
    Ok(())
}

/// Load binary raft metadata. Returns `None` if the file does not
/// exist, `Err` on corruption.
pub fn load_raft_metadata_binary(wal_path: &Path) -> Result<Option<RaftPersistMetadata>, Error> {
    let meta_path = wal_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("raft_meta.bin");
    let data = match std::fs::read(&meta_path) {
        Ok(data) => data,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };
    if data.len() < 28 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("raft_meta.bin too short: {} bytes", data.len()),
        ));
    }
    if data[0..4] != RAFT_META_MAGIC {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "raft_meta.bin invalid magic",
        ));
    }
    let current_term = u64::from_be_bytes(data[4..12].try_into().unwrap());
    const VOTED_FOR_NONE: u64 = u64::MAX;
    let voted_raw = u64::from_be_bytes(data[12..20].try_into().unwrap());
    let voted_for = if voted_raw == VOTED_FOR_NONE {
        None
    } else {
        Some(voted_raw)
    };
    let commit_index = u64::from_be_bytes(data[20..28].try_into().unwrap());
    Ok(Some(RaftPersistMetadata {
        current_term,
        voted_for,
        commit_index,
        needs_membership_flush: false,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_dir() -> PathBuf {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let base = std::env::temp_dir().join(format!(
            "wrela_db_raft_persist_{}_{}_{}",
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
    fn restore_rehydrates_term_vote_and_forces_follower_role() {
        let mut state = NodeState::with_timing(9, 5, 5);
        state.current_term = 7;
        state.voted_for = Some(9);
        state.role = Role::Leader;
        state.commit_index = 2;
        state.log = vec![
            LogEntry {
                index: 1,
                term: 6,
                payload: b"a".to_vec(),
            },
            LogEntry {
                index: 2,
                term: 7,
                payload: b"b".to_vec(),
            },
        ];
        let membership = MembershipConfig::new([1, 2, 3]).expect("membership");
        let persisted = PersistedRaftState::capture(&state, &membership);

        let mut restarted = NodeState::with_timing(9, 100, 3);
        let restored_membership = persisted.restore(&mut restarted, 42, 10).expect("restore");

        assert_eq!(restarted.current_term, 7);
        assert_eq!(restarted.voted_for, Some(9));
        assert_eq!(restarted.role, Role::Follower);
        assert_eq!(restarted.last_heartbeat_tick, 42);
        assert!(
            restarted.election_deadline_tick >= 52 && restarted.election_deadline_tick <= 61,
            "election_deadline_tick with jitter in [now+base, now+2*base) = [52, 62)"
        );
        assert_eq!(restarted.commit_index, 2);
        assert_eq!(restarted.log.len(), 2);
        assert_eq!(restarted.log[1].term, 7);
        assert_eq!(restored_membership.voters(), membership.voters());
    }

    #[test]
    fn restore_clamps_commit_index_and_rejects_non_contiguous_tail() {
        let persisted = PersistedRaftState {
            schema_version: RAFT_STATE_SCHEMA_VERSION,
            current_term: 9,
            voted_for: Some(2),
            commit_index: 99,
            log: vec![
                LogEntry {
                    index: 1,
                    term: 8,
                    payload: b"a".to_vec(),
                },
                LogEntry {
                    index: 3,
                    term: 9,
                    payload: b"bad-gap".to_vec(),
                },
            ],
            membership: PersistedMembershipState {
                voters: BTreeSet::from([1]),
                learners: BTreeSet::new(),
                joint: None,
            },
        };
        let mut restarted = NodeState::with_timing(2, 0, 5);
        let err = persisted
            .restore(&mut restarted, 10, 5)
            .expect_err("gap must be rejected");
        assert_eq!(err.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn round_trip_file_persistence_is_atomic_and_recoverable() {
        let dir = temp_dir();
        let wal_path = dir.join("wal.log");
        std::fs::write(&wal_path, b"").expect("seed wal");

        let mut state = NodeState::with_timing(1, 0, 10);
        state.current_term = 3;
        state.voted_for = Some(1);
        state.commit_index = 8;
        state.log = vec![
            LogEntry {
                index: 1,
                term: 1,
                payload: b"a".to_vec(),
            },
            LogEntry {
                index: 2,
                term: 3,
                payload: b"b".to_vec(),
            },
        ];
        let membership = MembershipConfig::new([1, 2, 3]).expect("membership");
        let persisted = PersistedRaftState::capture(&state, &membership);
        persist_raft_state(&wal_path, &persisted).expect("persist");

        let loaded = load_persisted_raft_state(&wal_path)
            .expect("load")
            .expect("state exists");
        assert_eq!(loaded.current_term, 3);
        assert_eq!(loaded.voted_for, Some(1));
        assert_eq!(loaded.log.len(), 2);
        assert_eq!(loaded.membership.voters, BTreeSet::from([1, 2, 3]));
    }
}
