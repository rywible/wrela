use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering as AtomicOrdering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use openraft::SnapshotMeta;

use super::blob::BlobBackend;
use super::config::{BackupConfig, RestoreMode};
use super::store::{NodeId, SerializableKvStateMachine};
use crate::metrics;
use openraft::BasicNode;
use sha2::{Digest, Sha256};

#[derive(Debug)]
pub struct BackupState {
    last_backup_at: AtomicU64,
    last_snapshot_index: AtomicU64,
    snapshot_inflight: AtomicBool,
    leader_id: AtomicU64,
}

impl BackupState {
    pub fn new() -> Self {
        Self {
            last_backup_at: AtomicU64::new(0),
            last_snapshot_index: AtomicU64::new(0),
            snapshot_inflight: AtomicBool::new(false),
            leader_id: AtomicU64::new(0),
        }
    }

    pub fn mark_now(&self) {
        let now = now_secs();
        self.last_backup_at.store(now, AtomicOrdering::Release);
    }

    pub fn last_backup_at(&self) -> u64 {
        self.last_backup_at.load(AtomicOrdering::Acquire)
    }

    pub fn last_snapshot_index(&self) -> u64 {
        self.last_snapshot_index.load(AtomicOrdering::Acquire)
    }

    pub fn set_last_snapshot_index(&self, index: u64) {
        self.last_snapshot_index
            .store(index, AtomicOrdering::Release);
    }

    pub fn snapshot_inflight(&self) -> bool {
        self.snapshot_inflight.load(AtomicOrdering::Acquire)
    }

    pub fn set_snapshot_inflight(&self, inflight: bool) {
        self.snapshot_inflight
            .store(inflight, AtomicOrdering::Release);
    }

    pub fn set_leader_id(&self, leader_id: Option<u64>) {
        self.leader_id
            .store(leader_id.unwrap_or(0), AtomicOrdering::Release);
    }

    pub fn leader_id(&self) -> Option<u64> {
        let id = self.leader_id.load(AtomicOrdering::Acquire);
        if id == 0 { None } else { Some(id) }
    }
}

#[derive(Clone, Debug)]
pub struct BackupSink {
    blob: BlobBackend,
    config: BackupConfig,
    state: Arc<BackupState>,
    node_id: u64,
}

impl BackupSink {
    pub fn new(
        blob: BlobBackend,
        config: BackupConfig,
        state: Arc<BackupState>,
        node_id: u64,
    ) -> Self {
        Self {
            blob,
            config,
            state,
            node_id,
        }
    }

    pub fn state(&self) -> Arc<BackupState> {
        self.state.clone()
    }

    pub fn on_snapshot(&self, meta: SnapshotMeta<NodeId, BasicNode>, data: Vec<u8>) {
        if !self.config.enabled {
            return;
        }
        let started = Instant::now();
        if let Some(log_id) = meta.last_log_id {
            self.state.set_last_snapshot_index(log_id.index);
        }
        if self.config.only_leader && self.state.leader_id() != Some(self.node_id) {
            self.state.set_snapshot_inflight(false);
            return;
        }
        self.state.set_snapshot_inflight(false);
        let blob = self.blob.clone();
        let config = self.config.clone();
        let state = self.state.clone();
        let node_id = self.node_id;
        tokio::spawn(async move {
            let key = backup_key(&config, node_id, &meta);
            match blob.put_named(&key, &data).await {
                Ok(_) => {
                    let checksum = checksum_hex(&data);
                    let checksum_key = checksum_key(&key);
                    if blob
                        .put_named(&checksum_key, checksum.as_bytes())
                        .await
                        .is_err()
                    {
                        metrics::inc_storage_backup_failure();
                        return;
                    }
                    state.mark_now();
                    metrics::inc_storage_backup_success();
                    metrics::record_storage_backup_duration(started.elapsed());
                    metrics::record_storage_backup_size(data.len());
                    metrics::record_storage_backup_ts(now_secs());
                }
                Err(_) => {
                    metrics::inc_storage_backup_failure();
                }
            }
            let _ = prune_backups(&blob, &config, node_id).await;
        });
    }
}

pub async fn latest_backup_key(
    blob: &BlobBackend,
    config: &BackupConfig,
    node_id: u64,
) -> Option<String> {
    let prefix = backup_prefix(config, node_id);
    let list = blob.list_prefix(&prefix).await.ok()?;
    let mut keys: Vec<String> = list
        .into_iter()
        .map(|b| b.key)
        .filter(|k| k.ends_with(".snap"))
        .collect();
    keys.sort();
    keys.pop()
}

pub async fn verify_checksum(
    blob: &BlobBackend,
    key: &str,
    data: &[u8],
    strict: bool,
) -> Result<(), String> {
    let checksum_key = checksum_key(key);
    let expected = match blob.get_named(&checksum_key).await {
        Ok(bytes) => String::from_utf8(bytes).map_err(|err| err.to_string())?,
        Err(_) => {
            if strict {
                return Err("checksum missing".to_string());
            }
            return Ok(());
        }
    };
    let actual = checksum_hex(data);
    if expected.trim() != actual {
        return Err("checksum mismatch".to_string());
    }
    Ok(())
}

pub fn should_restore(config: &BackupConfig) -> bool {
    matches!(
        config.restore_mode,
        RestoreMode::Single | RestoreMode::Cluster
    )
}

pub fn backup_prefix(config: &BackupConfig, node_id: u64) -> String {
    let prefix = config.prefix.trim_matches('/');
    if prefix.is_empty() {
        format!("backups/{node_id}")
    } else {
        format!("{}/{node_id}", prefix)
    }
}

fn backup_key(
    config: &BackupConfig,
    node_id: u64,
    meta: &SnapshotMeta<NodeId, BasicNode>,
) -> String {
    let ts = now_millis();
    let (term, index) = meta
        .last_log_id
        .map(|id| (id.leader_id.term, id.index))
        .unwrap_or((0, 0));
    let prefix = backup_prefix(config, node_id);
    format!("{}/{:013}-{term}-{index}.snap", prefix, ts)
}

async fn prune_backups(
    blob: &BlobBackend,
    config: &BackupConfig,
    node_id: u64,
) -> Result<(), String> {
    if config.retention_days == 0 && config.max_keep == 0 {
        return Ok(());
    }
    let prefix = backup_prefix(config, node_id);
    let mut list = blob
        .list_prefix(&prefix)
        .await
        .map_err(|err| err.to_string())?;
    list.retain(|entry| entry.key.ends_with(".snap"));
    list.sort_by(|a, b| a.key.cmp(&b.key));

    let mut to_delete: Vec<String> = Vec::new();

    if config.retention_days > 0 {
        let cutoff = now_secs().saturating_sub(config.retention_days * 24 * 3600);
        for entry in list.iter() {
            if let Some(ts) = parse_timestamp(&entry.key) {
                if ts / 1000 < cutoff {
                    to_delete.push(entry.key.clone());
                }
            }
        }
    }

    if config.max_keep > 0 && list.len() > config.max_keep {
        let keep_from = list.len() - config.max_keep;
        for entry in list.iter().take(keep_from) {
            to_delete.push(entry.key.clone());
        }
    }

    for key in to_delete {
        let checksum = checksum_key(&key);
        let _ = blob.delete_key(&key).await;
        let _ = blob.delete_key(&checksum).await;
    }

    Ok(())
}

#[cfg(test)]
pub(crate) async fn prune_backups_for_test(
    blob: &BlobBackend,
    config: &BackupConfig,
    node_id: u64,
) -> Result<(), String> {
    prune_backups(blob, config, node_id).await
}

fn parse_timestamp(key: &str) -> Option<u64> {
    let filename = key.rsplit('/').next()?;
    let ts_str = filename.split('-').next()?;
    ts_str.parse::<u64>().ok()
}

pub fn snapshot_meta_from_bytes(
    snapshot_id: String,
    data: &[u8],
) -> Result<SnapshotMeta<NodeId, BasicNode>, String> {
    let state: SerializableKvStateMachine =
        serde_json::from_slice(data).map_err(|err| err.to_string())?;
    Ok(SnapshotMeta {
        last_log_id: state.last_applied_log,
        last_membership: state.last_membership,
        snapshot_id,
    })
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis() as u64
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs()
}

fn checksum_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    format!("{:x}", digest)
}

fn checksum_key(key: &str) -> String {
    format!("{key}.sha256")
}
