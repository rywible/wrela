use crate::db::raft::persistence::PersistedRaftState;
use crate::db::shard::directory::ShardDirectorySnapshot;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, Error, ErrorKind, Write};
use std::path::{Path, PathBuf};

const TOPOLOGY_STATE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedAutoscaleStatus {
    pub last_action: String,
    pub reasons: Vec<String>,
    pub last_action_at_epoch_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedGroupState {
    pub group_id: u32,
    pub raft: PersistedRaftState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedTopologyState {
    pub schema_version: u32,
    pub shard_directory: ShardDirectorySnapshot,
    pub groups: Vec<PersistedGroupState>,
    pub replication_factor: u32,
    pub write_quorum: u32,
    pub autoscale_status: Option<PersistedAutoscaleStatus>,
}

impl PersistedTopologyState {
    pub fn new(
        shard_directory: ShardDirectorySnapshot,
        groups: Vec<PersistedGroupState>,
        replication_factor: u32,
        write_quorum: u32,
        autoscale_status: Option<PersistedAutoscaleStatus>,
    ) -> Self {
        Self {
            schema_version: TOPOLOGY_STATE_SCHEMA_VERSION,
            shard_directory,
            groups,
            replication_factor,
            write_quorum,
            autoscale_status,
        }
    }

    pub fn validate(&self) -> Result<(), Error> {
        if self.schema_version != TOPOLOGY_STATE_SCHEMA_VERSION {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "unsupported topology schema version {}; expected {}",
                    self.schema_version, TOPOLOGY_STATE_SCHEMA_VERSION
                ),
            ));
        }
        if self.shard_directory.active_group_count == 0 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "topology active_group_count must be > 0",
            ));
        }
        if self.groups.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "topology must include at least one group",
            ));
        }
        if self.replication_factor == 0 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "replication_factor must be > 0",
            ));
        }
        if self.write_quorum == 0 || self.write_quorum > self.replication_factor {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "write_quorum must be in [1, replication_factor]",
            ));
        }
        Ok(())
    }
}

pub fn topology_state_path_from(wal_path: &Path) -> PathBuf {
    wal_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("topology_state.json")
}

pub fn load_persisted_topology_state(
    wal_path: &Path,
) -> Result<Option<PersistedTopologyState>, Error> {
    let path = topology_state_path_from(wal_path);
    match File::open(&path) {
        Ok(file) => {
            let state = serde_json::from_reader::<_, PersistedTopologyState>(BufReader::new(file))
                .map_err(|err| Error::new(ErrorKind::InvalidData, err.to_string()))?;
            state.validate()?;
            Ok(Some(state))
        }
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

pub fn persist_topology_state(
    wal_path: &Path,
    state: &PersistedTopologyState,
) -> Result<(), Error> {
    state.validate()?;
    let path = topology_state_path_from(wal_path);
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

fn fsync_parent_dir(path: &Path) -> Result<(), Error> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let dir = File::open(parent)?;
    dir.sync_all()
}
