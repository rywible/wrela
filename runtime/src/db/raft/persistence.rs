use crate::db::raft::membership::{JointMembership, MembershipConfig};
use crate::db::raft::message::LogEntry;
use crate::db::raft::state::{NodeState, Role};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::io::{Error, ErrorKind, Write};
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
    match std::fs::read(&path) {
        Ok(payload) => serde_json::from_slice::<PersistedRaftState>(&payload)
            .map(Some)
            .map_err(|err| Error::new(ErrorKind::InvalidData, err.to_string())),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

pub fn persist_raft_state(wal_path: &Path, state: &PersistedRaftState) -> Result<(), Error> {
    ensure_contiguous_log(&state.log)?;
    let payload = serde_json::to_vec_pretty(state)
        .map_err(|err| Error::new(ErrorKind::InvalidData, err.to_string()))?;

    let path = raft_state_path_from(wal_path);
    let tmp = path.with_extension("json.tmp");
    {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&tmp)?;
        file.write_all(&payload)?;
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
        assert_eq!(restarted.election_deadline_tick, 52);
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
