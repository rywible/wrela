pub mod file_store;
pub mod s3_store;
pub mod store;

use crate::db::checkpoint::file_store::FileCheckpointStore;
use crate::db::checkpoint::store::CheckpointStore;
use crate::db::snapshot::checksum::checksum;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

pub const CHECKPOINT_MANIFEST_VERSION: u32 = 1;
pub const DEFAULT_LOCAL_RETENTION: usize = 3;

const WAL_FILE: &str = "wal.log";
const RAFT_FILE: &str = "raft_state.json";
const HLC_FILE: &str = "hlc_state.json";
const CDC_FILE: &str = "cdc_checkpoints.json";
const SCHEMA_EPOCH_FILE: &str = "schema_epoch.json";
const SNAPSHOT_FILE: &str = "snapshot.bin";
const MANIFEST_FILE: &str = "manifest.json";
const LATEST_POINTER_FILE: &str = "LATEST";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointArtifact {
    pub name: String,
    pub checksum: u64,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointManifest {
    pub version: u32,
    pub checkpoint_id: String,
    pub created_at_epoch_s: u64,
    pub artifacts: Vec<CheckpointArtifact>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointInfo {
    pub checkpoint_id: String,
    pub created_at_epoch_s: u64,
}

#[derive(Clone)]
pub struct CheckpointManager {
    local_store: FileCheckpointStore,
    remote_store: Option<
        std::sync::Arc<
            dyn CheckpointStore<Error = crate::db::checkpoint::s3_store::S3CheckpointError>,
        >,
    >,
    retention: usize,
    /// Serializes concurrent create_checkpoint and prune_local operations.
    op_lock: std::sync::Arc<std::sync::Mutex<()>>,
}

impl fmt::Debug for CheckpointManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CheckpointManager")
            .field("retention", &self.retention)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckpointError {
    Io(String),
    Serde(String),
    Corrupt(String),
    Missing(String),
    Remote(String),
}

impl fmt::Display for CheckpointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(msg) => write!(f, "checkpoint io error: {msg}"),
            Self::Serde(msg) => write!(f, "checkpoint serde error: {msg}"),
            Self::Corrupt(msg) => write!(f, "checkpoint corrupt: {msg}"),
            Self::Missing(msg) => write!(f, "checkpoint missing: {msg}"),
            Self::Remote(msg) => write!(f, "checkpoint remote: {msg}"),
        }
    }
}

impl std::error::Error for CheckpointError {}

impl CheckpointManager {
    pub fn new(local_root: impl Into<PathBuf>, retention: usize) -> Self {
        Self {
            local_store: FileCheckpointStore::new(local_root),
            remote_store: None,
            retention: retention.max(1),
            op_lock: std::sync::Arc::new(std::sync::Mutex::new(())),
        }
    }

    pub fn with_remote_store(
        mut self,
        remote_store: std::sync::Arc<
            dyn CheckpointStore<Error = crate::db::checkpoint::s3_store::S3CheckpointError>,
        >,
    ) -> Self {
        self.remote_store = Some(remote_store);
        self
    }

    pub fn create_checkpoint(&self, data_dir: &Path) -> Result<CheckpointInfo, CheckpointError> {
        let _guard = self.op_lock.lock().expect("checkpoint op lock");
        let checkpoint_id = format!(
            "{}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            std::process::id()
        );
        let files = [
            WAL_FILE,
            RAFT_FILE,
            HLC_FILE,
            CDC_FILE,
            SCHEMA_EPOCH_FILE,
            SNAPSHOT_FILE,
        ];

        // Phase 1: Write all artifacts (local + remote).
        let mut artifacts = Vec::new();
        for file in files {
            let path = data_dir.join(file);
            if !path.exists() {
                continue;
            }
            let payload =
                std::fs::read(&path).map_err(|err| CheckpointError::Io(err.to_string()))?;
            let key = format!("checkpoints/{checkpoint_id}/{file}");
            self.local_store
                .put_object(&key, &payload)
                .map_err(|err| CheckpointError::Io(err.to_string()))?;
            if let Some(remote) = &self.remote_store {
                remote
                    .put_object(&key, &payload)
                    .map_err(|err| CheckpointError::Remote(err.to_string()))?;
            }
            artifacts.push(CheckpointArtifact {
                name: file.to_string(),
                checksum: checksum(&payload),
                size_bytes: payload.len() as u64,
            });
        }

        // Phase 2: Write manifests (local + remote) before LATEST pointers.
        let created_at_epoch_s = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|dur| dur.as_secs())
            .unwrap_or(0);
        let manifest = CheckpointManifest {
            version: CHECKPOINT_MANIFEST_VERSION,
            checkpoint_id: checkpoint_id.clone(),
            created_at_epoch_s,
            artifacts,
        };
        let manifest_payload = serde_json::to_vec_pretty(&manifest)
            .map_err(|err| CheckpointError::Serde(err.to_string()))?;
        let manifest_key = format!("checkpoints/{checkpoint_id}/{MANIFEST_FILE}");
        self.local_store
            .put_object(&manifest_key, &manifest_payload)
            .map_err(|err| CheckpointError::Io(err.to_string()))?;
        if let Some(remote) = &self.remote_store {
            remote
                .put_object(&manifest_key, &manifest_payload)
                .map_err(|err| CheckpointError::Remote(err.to_string()))?;
        }

        // Phase 3: Write LATEST pointers (local + remote) last.
        self.local_store
            .put_object(LATEST_POINTER_FILE, checkpoint_id.as_bytes())
            .map_err(|err| CheckpointError::Io(err.to_string()))?;
        if let Some(remote) = &self.remote_store {
            remote
                .put_object(LATEST_POINTER_FILE, checkpoint_id.as_bytes())
                .map_err(|err| CheckpointError::Remote(err.to_string()))?;
        }

        self.prune_local_inner()?;

        Ok(CheckpointInfo {
            checkpoint_id,
            created_at_epoch_s,
        })
    }

    pub fn restore_latest(&self, data_dir: &Path) -> Result<CheckpointInfo, CheckpointError> {
        self.reconcile_latest_pointers();
        let mut errors = Vec::new();
        let mut local_manifests = self.collect_local_manifests()?;
        self.prioritize_latest_pointer(&mut local_manifests);
        for manifest in local_manifests {
            match self.restore_from_manifest(data_dir, &manifest, true) {
                Ok(()) => {
                    return Ok(CheckpointInfo {
                        checkpoint_id: manifest.checkpoint_id,
                        created_at_epoch_s: manifest.created_at_epoch_s,
                    });
                }
                Err(err) => errors.push(format!("local:{}:{err}", manifest.checkpoint_id)),
            }
        }

        if let Some(remote) = &self.remote_store {
            let mut remote_manifests = self.collect_remote_manifests(remote)?;
            self.prioritize_latest_pointer_remote(remote, &mut remote_manifests);
            for manifest in remote_manifests {
                match self.restore_from_manifest(data_dir, &manifest, true) {
                    Ok(()) => {
                        let manifest_key =
                            format!("checkpoints/{}/{MANIFEST_FILE}", manifest.checkpoint_id);
                        let payload = serde_json::to_vec_pretty(&manifest)
                            .map_err(|err| CheckpointError::Serde(err.to_string()))?;
                        if let Err(err) = self.local_store.put_object(&manifest_key, &payload) {
                            eprintln!("wrela: checkpoint: failed to cache manifest locally: {err}");
                        }
                        if let Err(err) = self
                            .local_store
                            .put_object(LATEST_POINTER_FILE, manifest.checkpoint_id.as_bytes())
                        {
                            eprintln!(
                                "wrela: checkpoint: failed to cache LATEST pointer locally: {err}"
                            );
                        }
                        return Ok(CheckpointInfo {
                            checkpoint_id: manifest.checkpoint_id,
                            created_at_epoch_s: manifest.created_at_epoch_s,
                        });
                    }
                    Err(err) => errors.push(format!("remote:{}:{err}", manifest.checkpoint_id)),
                }
            }
        }

        Err(CheckpointError::Missing(format!(
            "no recoverable checkpoint found: {}",
            errors.join("; ")
        )))
    }

    pub fn restore_checkpoint(
        &self,
        data_dir: &Path,
        checkpoint_id: &str,
    ) -> Result<CheckpointInfo, CheckpointError> {
        let checkpoint_id = checkpoint_id.trim();
        if checkpoint_id.is_empty() {
            return Err(CheckpointError::Missing(
                "checkpoint id must be non-empty".to_string(),
            ));
        }
        let _guard = self.op_lock.lock().expect("checkpoint op lock");
        let mut errors = Vec::new();
        let manifest_key = format!("checkpoints/{checkpoint_id}/{MANIFEST_FILE}");

        if let Ok(payload) = self.local_store.get_object(&manifest_key) {
            match Self::parse_manifest(&payload) {
                Ok(manifest) => {
                    if manifest.checkpoint_id != checkpoint_id {
                        errors.push(format!(
                            "local:{checkpoint_id}:manifest id mismatch {}",
                            manifest.checkpoint_id
                        ));
                    } else {
                        match self.restore_from_manifest(data_dir, &manifest, true) {
                            Ok(()) => {
                                let _ = self
                                    .local_store
                                    .put_object(LATEST_POINTER_FILE, checkpoint_id.as_bytes());
                                if let Some(remote) = &self.remote_store {
                                    let _ = remote
                                        .put_object(LATEST_POINTER_FILE, checkpoint_id.as_bytes());
                                }
                                return Ok(CheckpointInfo {
                                    checkpoint_id: manifest.checkpoint_id,
                                    created_at_epoch_s: manifest.created_at_epoch_s,
                                });
                            }
                            Err(err) => {
                                errors.push(format!("local:{checkpoint_id}:{err}"));
                            }
                        }
                    }
                }
                Err(err) => errors.push(format!("local:{checkpoint_id}:{err}")),
            }
        } else {
            errors.push(format!("local:{checkpoint_id}:manifest missing"));
        }

        if let Some(remote) = &self.remote_store {
            match remote.get_object(&manifest_key) {
                Ok(payload) => match Self::parse_manifest(&payload) {
                    Ok(manifest) => {
                        if manifest.checkpoint_id != checkpoint_id {
                            errors.push(format!(
                                "remote:{checkpoint_id}:manifest id mismatch {}",
                                manifest.checkpoint_id
                            ));
                        } else {
                            match self.restore_from_manifest(data_dir, &manifest, true) {
                                Ok(()) => {
                                    let _ = self.local_store.put_object(&manifest_key, &payload);
                                    let _ = self
                                        .local_store
                                        .put_object(LATEST_POINTER_FILE, checkpoint_id.as_bytes());
                                    let _ = remote
                                        .put_object(LATEST_POINTER_FILE, checkpoint_id.as_bytes());
                                    return Ok(CheckpointInfo {
                                        checkpoint_id: manifest.checkpoint_id,
                                        created_at_epoch_s: manifest.created_at_epoch_s,
                                    });
                                }
                                Err(err) => {
                                    errors.push(format!("remote:{checkpoint_id}:{err}"));
                                }
                            }
                        }
                    }
                    Err(err) => errors.push(format!("remote:{checkpoint_id}:{err}")),
                },
                Err(err) => errors.push(format!("remote:{checkpoint_id}:{err}")),
            }
        }

        Err(CheckpointError::Missing(format!(
            "checkpoint `{checkpoint_id}` not recoverable: {}",
            errors.join("; ")
        )))
    }

    pub fn list_checkpoints(&self) -> Result<Vec<CheckpointInfo>, CheckpointError> {
        let keys = self
            .local_store
            .list_prefix("checkpoints")
            .map_err(|err| CheckpointError::Io(err.to_string()))?;
        let mut out = Vec::new();
        for key in keys {
            if !key.ends_with(&format!("/{MANIFEST_FILE}")) {
                continue;
            }
            let payload = self
                .local_store
                .get_object(&key)
                .map_err(|err| CheckpointError::Io(err.to_string()))?;
            let manifest: CheckpointManifest = serde_json::from_slice(&payload)
                .map_err(|err| CheckpointError::Serde(err.to_string()))?;
            out.push(CheckpointInfo {
                checkpoint_id: manifest.checkpoint_id,
                created_at_epoch_s: manifest.created_at_epoch_s,
            });
        }
        out.sort_by(|a, b| a.created_at_epoch_s.cmp(&b.created_at_epoch_s));
        Ok(out)
    }

    pub fn prune_local(&self) -> Result<(), CheckpointError> {
        let _guard = self.op_lock.lock().expect("checkpoint op lock");
        self.prune_local_inner()
    }

    fn prune_local_inner(&self) -> Result<(), CheckpointError> {
        let mut list = self.list_checkpoints()?;
        if list.len() <= self.retention {
            return Ok(());
        }
        list.sort_by(|a, b| a.created_at_epoch_s.cmp(&b.created_at_epoch_s));
        let to_delete = list.len().saturating_sub(self.retention);
        for info in list.into_iter().take(to_delete) {
            let prefix = format!("checkpoints/{}/", info.checkpoint_id);
            let keys = self
                .local_store
                .list_prefix(&prefix)
                .map_err(|err| CheckpointError::Io(err.to_string()))?;
            for key in keys {
                self.local_store
                    .delete_object(&key)
                    .map_err(|err| CheckpointError::Io(err.to_string()))?;
            }
        }
        Ok(())
    }

    pub fn prune_remote(&self, retain: usize) -> Result<(), CheckpointError> {
        let Some(remote) = &self.remote_store else {
            return Ok(());
        };
        let mut manifests = Vec::new();
        let keys = remote
            .list_prefix("checkpoints/")
            .map_err(|err| CheckpointError::Remote(err.to_string()))?;
        for key in keys {
            if !key.ends_with(&format!("/{MANIFEST_FILE}")) {
                continue;
            }
            let payload = remote
                .get_object(&key)
                .map_err(|err| CheckpointError::Remote(err.to_string()))?;
            let manifest: CheckpointManifest = serde_json::from_slice(&payload)
                .map_err(|err| CheckpointError::Serde(err.to_string()))?;
            manifests.push(manifest);
        }
        manifests.sort_by(|a, b| a.created_at_epoch_s.cmp(&b.created_at_epoch_s));
        let drop_n = manifests.len().saturating_sub(retain.max(1));
        for manifest in manifests.into_iter().take(drop_n) {
            let prefix = format!("checkpoints/{}/", manifest.checkpoint_id);
            let keys = remote
                .list_prefix(&prefix)
                .map_err(|err| CheckpointError::Remote(err.to_string()))?;
            for key in keys {
                remote
                    .delete_object(&key)
                    .map_err(|err| CheckpointError::Remote(err.to_string()))?;
            }
        }
        Ok(())
    }

    /// Reconcile divergent LATEST pointers between local and remote stores.
    /// Reads both LATEST pointers, resolves the manifest for each, and writes
    /// the newer checkpoint ID to whichever side is lagging.
    fn reconcile_latest_pointers(&self) {
        let Some(remote) = &self.remote_store else {
            return;
        };
        let local_id = self
            .local_store
            .get_object(LATEST_POINTER_FILE)
            .ok()
            .and_then(|v| String::from_utf8(v).ok())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        let remote_id = remote
            .get_object(LATEST_POINTER_FILE)
            .ok()
            .and_then(|v| String::from_utf8(v).ok())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        let (local_id, remote_id) = match (local_id, remote_id) {
            (Some(l), Some(r)) if l == r => return,
            (Some(l), Some(r)) => (l, r),
            (None, Some(r)) => {
                let manifest_key = format!("checkpoints/{r}/{MANIFEST_FILE}");
                if let Ok(payload) = remote.get_object(&manifest_key) {
                    let _ = self.local_store.put_object(&manifest_key, &payload);
                    let _ = self
                        .local_store
                        .put_object(LATEST_POINTER_FILE, r.as_bytes());
                }
                return;
            }
            (Some(l), None) => {
                let manifest_key = format!("checkpoints/{l}/{MANIFEST_FILE}");
                if let Ok(payload) = self.local_store.get_object(&manifest_key) {
                    let _ = remote.put_object(&manifest_key, &payload);
                    let _ = remote.put_object(LATEST_POINTER_FILE, l.as_bytes());
                }
                return;
            }
            (None, None) => return,
        };
        let local_ts = self
            .local_store
            .get_object(&format!("checkpoints/{local_id}/{MANIFEST_FILE}"))
            .ok()
            .and_then(|p| Self::parse_manifest(&p).ok())
            .map(|m| m.created_at_epoch_s)
            .unwrap_or(0);
        let remote_ts = remote
            .get_object(&format!("checkpoints/{remote_id}/{MANIFEST_FILE}"))
            .ok()
            .and_then(|p| Self::parse_manifest(&p).ok())
            .map(|m| m.created_at_epoch_s)
            .unwrap_or(0);
        if remote_ts > local_ts {
            let _ = self
                .local_store
                .put_object(LATEST_POINTER_FILE, remote_id.as_bytes());
            // Also cache the remote manifest locally for faster restore.
            let manifest_key = format!("checkpoints/{remote_id}/{MANIFEST_FILE}");
            if let Ok(payload) = remote.get_object(&manifest_key) {
                let _ = self.local_store.put_object(&manifest_key, &payload);
            }
        } else {
            let _ = remote.put_object(LATEST_POINTER_FILE, local_id.as_bytes());
            let manifest_key = format!("checkpoints/{local_id}/{MANIFEST_FILE}");
            if let Ok(payload) = self.local_store.get_object(&manifest_key) {
                let _ = remote.put_object(&manifest_key, &payload);
            }
        }
    }

    fn parse_manifest(payload: &[u8]) -> Result<CheckpointManifest, CheckpointError> {
        let manifest: CheckpointManifest = serde_json::from_slice(&payload)
            .map_err(|err| CheckpointError::Serde(err.to_string()))?;
        if manifest.version != CHECKPOINT_MANIFEST_VERSION {
            return Err(CheckpointError::Corrupt(format!(
                "unsupported manifest version {}",
                manifest.version
            )));
        }
        Ok(manifest)
    }

    fn collect_local_manifests(&self) -> Result<Vec<CheckpointManifest>, CheckpointError> {
        let keys = self
            .local_store
            .list_prefix("checkpoints")
            .map_err(|err| CheckpointError::Io(err.to_string()))?;
        let mut manifests = Vec::new();
        for key in keys {
            if !key.ends_with(&format!("/{MANIFEST_FILE}")) {
                continue;
            }
            let payload = match self.local_store.get_object(&key) {
                Ok(payload) => payload,
                Err(_) => continue,
            };
            if let Ok(manifest) = Self::parse_manifest(&payload) {
                manifests.push(manifest);
            }
        }
        manifests.sort_by(|a, b| b.created_at_epoch_s.cmp(&a.created_at_epoch_s));
        manifests.dedup_by(|a, b| a.checkpoint_id == b.checkpoint_id);
        Ok(manifests)
    }

    fn collect_remote_manifests(
        &self,
        remote: &std::sync::Arc<
            dyn CheckpointStore<Error = crate::db::checkpoint::s3_store::S3CheckpointError>,
        >,
    ) -> Result<Vec<CheckpointManifest>, CheckpointError> {
        let keys = remote
            .list_prefix("checkpoints/")
            .map_err(|err| CheckpointError::Remote(err.to_string()))?;
        let mut manifests = Vec::new();
        for key in keys {
            if !key.ends_with(&format!("/{MANIFEST_FILE}")) {
                continue;
            }
            let payload = match remote.get_object(&key) {
                Ok(payload) => payload,
                Err(_) => continue,
            };
            if let Ok(manifest) = Self::parse_manifest(&payload) {
                manifests.push(manifest);
            }
        }
        manifests.sort_by(|a, b| b.created_at_epoch_s.cmp(&a.created_at_epoch_s));
        manifests.dedup_by(|a, b| a.checkpoint_id == b.checkpoint_id);
        Ok(manifests)
    }

    fn prioritize_latest_pointer(&self, manifests: &mut Vec<CheckpointManifest>) {
        let pointer = self
            .local_store
            .get_object(LATEST_POINTER_FILE)
            .ok()
            .and_then(|v| String::from_utf8(v).ok())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        if let Some(pointer_id) = pointer {
            if let Some(idx) = manifests.iter().position(|m| m.checkpoint_id == pointer_id) {
                let manifest = manifests.remove(idx);
                manifests.insert(0, manifest);
            }
        }
    }

    fn prioritize_latest_pointer_remote(
        &self,
        remote: &std::sync::Arc<
            dyn CheckpointStore<Error = crate::db::checkpoint::s3_store::S3CheckpointError>,
        >,
        manifests: &mut Vec<CheckpointManifest>,
    ) {
        let pointer = remote
            .get_object(LATEST_POINTER_FILE)
            .ok()
            .and_then(|v| String::from_utf8(v).ok())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        if let Some(pointer_id) = pointer {
            if let Some(idx) = manifests.iter().position(|m| m.checkpoint_id == pointer_id) {
                let manifest = manifests.remove(idx);
                manifests.insert(0, manifest);
            }
        }
    }

    fn restore_from_manifest(
        &self,
        data_dir: &Path,
        manifest: &CheckpointManifest,
        allow_remote_fallback: bool,
    ) -> Result<(), CheckpointError> {
        std::fs::create_dir_all(data_dir).map_err(|err| CheckpointError::Io(err.to_string()))?;

        // Restore into a staging directory first so a failure partway through
        // does not leave data_dir in a partially-restored state.
        let staging_dir = data_dir.join(format!(".restore-staging-{}", manifest.checkpoint_id));
        if staging_dir.exists() {
            std::fs::remove_dir_all(&staging_dir)
                .map_err(|err| CheckpointError::Io(err.to_string()))?;
        }
        std::fs::create_dir_all(&staging_dir)
            .map_err(|err| CheckpointError::Io(err.to_string()))?;

        let result = self.restore_artifacts_to(&staging_dir, manifest, allow_remote_fallback);

        if result.is_err() {
            let _ = std::fs::remove_dir_all(&staging_dir);
            return result;
        }

        // Move staged artifacts into data_dir atomically (per-file rename).
        for artifact in &manifest.artifacts {
            let src = staging_dir.join(&artifact.name);
            let dst = data_dir.join(&artifact.name);
            std::fs::rename(&src, &dst).map_err(|err| CheckpointError::Io(err.to_string()))?;
        }
        let _ = std::fs::remove_dir_all(&staging_dir);
        Ok(())
    }

    fn restore_artifacts_to(
        &self,
        target_dir: &Path,
        manifest: &CheckpointManifest,
        allow_remote_fallback: bool,
    ) -> Result<(), CheckpointError> {
        for artifact in &manifest.artifacts {
            let key = format!(
                "checkpoints/{}/{name}",
                manifest.checkpoint_id,
                name = artifact.name
            );
            let mut payload = self.local_store.get_object(&key).ok();
            if payload.is_none() && allow_remote_fallback {
                payload = self.fetch_remote_artifact(&key)?;
            }
            let mut payload = payload.ok_or_else(|| {
                CheckpointError::Missing(format!("missing artifact for {}", artifact.name))
            })?;
            let mut actual = checksum(&payload);
            if actual != artifact.checksum && allow_remote_fallback {
                if let Some(remote_payload) = self.fetch_remote_artifact(&key)? {
                    let remote_checksum = checksum(&remote_payload);
                    if remote_checksum == artifact.checksum {
                        payload = remote_payload;
                        actual = remote_checksum;
                    }
                }
            }
            if actual != artifact.checksum {
                return Err(CheckpointError::Corrupt(format!(
                    "artifact checksum mismatch file={} expected={} actual={}",
                    artifact.name, artifact.checksum, actual
                )));
            }
            std::fs::write(target_dir.join(&artifact.name), payload)
                .map_err(|err| CheckpointError::Io(err.to_string()))?;
        }
        Ok(())
    }

    fn fetch_remote_artifact(&self, key: &str) -> Result<Option<Vec<u8>>, CheckpointError> {
        let Some(remote) = &self.remote_store else {
            return Ok(None);
        };
        match remote.get_object(key) {
            Ok(payload) => {
                if let Err(err) = self.local_store.put_object(key, &payload) {
                    eprintln!("wrela: checkpoint: failed to cache artifact locally ({key}): {err}");
                }
                Ok(Some(payload))
            }
            Err(_) => Ok(None),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointBackend {
    File,
    S3,
}

#[derive(Debug, Clone)]
pub struct CheckpointConfig {
    pub backend: CheckpointBackend,
    pub checkpoint_dir: PathBuf,
    pub local_region: Option<String>,
    pub s3_bucket: Option<String>,
    pub s3_prefix: Option<String>,
    pub s3_region: Option<String>,
    pub s3_endpoint: Option<String>,
    pub s3_path_style: bool,
    pub s3_bucket_by_region: BTreeMap<String, String>,
    pub s3_region_by_region: BTreeMap<String, String>,
    pub s3_endpoint_by_region: BTreeMap<String, String>,
    pub env_parse_error: Option<String>,
    pub interval_secs: u64,
    pub retain_local: usize,
    pub allowed_regions: Vec<String>,
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            backend: CheckpointBackend::File,
            checkpoint_dir: PathBuf::from(".checkpoints"),
            local_region: None,
            s3_bucket: None,
            s3_prefix: None,
            s3_region: None,
            s3_endpoint: None,
            s3_path_style: false,
            s3_bucket_by_region: BTreeMap::new(),
            s3_region_by_region: BTreeMap::new(),
            s3_endpoint_by_region: BTreeMap::new(),
            env_parse_error: None,
            interval_secs: 60,
            retain_local: DEFAULT_LOCAL_RETENTION,
            allowed_regions: Vec::new(),
        }
    }
}

impl CheckpointConfig {
    pub fn from_env() -> Self {
        let mut cfg = Self::default();
        if let Ok(raw) = std::env::var("WRELADB_CHECKPOINT_BACKEND") {
            cfg.backend = if raw.eq_ignore_ascii_case("s3") {
                CheckpointBackend::S3
            } else {
                CheckpointBackend::File
            };
        }
        if let Ok(dir) = std::env::var("WRELADB_CHECKPOINT_DIR") {
            cfg.checkpoint_dir = PathBuf::from(dir);
        }
        cfg.s3_bucket = std::env::var("WRELADB_S3_BUCKET").ok();
        cfg.s3_prefix = std::env::var("WRELADB_S3_PREFIX").ok();
        cfg.s3_region = std::env::var("WRELADB_S3_REGION").ok();
        cfg.s3_endpoint = std::env::var("WRELADB_S3_ENDPOINT").ok();
        cfg.local_region = local_region_from_env();
        cfg.s3_path_style = std::env::var("WRELADB_S3_PATH_STYLE")
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        match parse_region_string_map_env("WRELADB_S3_BUCKET_BY_REGION_JSON") {
            Ok(parsed) => cfg.s3_bucket_by_region = parsed,
            Err(err) => cfg.env_parse_error = Some(err),
        }
        match parse_region_string_map_env("WRELADB_S3_REGION_BY_REGION_JSON") {
            Ok(parsed) => cfg.s3_region_by_region = parsed,
            Err(err) => {
                if cfg.env_parse_error.is_none() {
                    cfg.env_parse_error = Some(err);
                }
            }
        };
        match parse_region_string_map_env("WRELADB_S3_ENDPOINT_BY_REGION_JSON") {
            Ok(parsed) => cfg.s3_endpoint_by_region = parsed,
            Err(err) => {
                if cfg.env_parse_error.is_none() {
                    cfg.env_parse_error = Some(err);
                }
            }
        };
        cfg.apply_region_local_overrides();
        cfg.interval_secs = std::env::var("WRELADB_CHECKPOINT_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(60)
            .max(1);
        cfg.retain_local = std::env::var("WRELADB_CHECKPOINT_RETAIN_LOCAL")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(DEFAULT_LOCAL_RETENTION)
            .max(1);
        cfg
    }

    pub fn build_manager(&self) -> Result<CheckpointManager, CheckpointError> {
        if let Some(err) = self.env_parse_error.as_deref() {
            return Err(CheckpointError::Missing(format!(
                "invalid checkpoint env config: {err}"
            )));
        }
        let manager = CheckpointManager::new(&self.checkpoint_dir, self.retain_local);
        match self.backend {
            CheckpointBackend::File => Ok(manager),
            CheckpointBackend::S3 => {
                let bucket = self
                    .s3_bucket
                    .clone()
                    .ok_or_else(|| {
                        let region = self.local_region.clone().unwrap_or_else(|| "unknown".to_string());
                        CheckpointError::Missing(format!(
                            "WRELADB_S3_BUCKET (or WRELADB_S3_BUCKET_BY_REGION_JSON entry for region `{region}`)"
                        ))
                    })?;
                let prefix = self
                    .s3_prefix
                    .clone()
                    .unwrap_or_else(|| "wreladb/checkpoints".to_string());
                let region = self
                    .s3_region
                    .clone()
                    .ok_or_else(|| {
                        let local = self.local_region.clone().unwrap_or_else(|| "unknown".to_string());
                        CheckpointError::Missing(format!(
                            "WRELADB_S3_REGION (or WRELADB_S3_REGION_BY_REGION_JSON entry for region `{local}`)"
                        ))
                    })?;
                let store = crate::db::checkpoint::s3_store::S3CheckpointStore::from_config(
                    crate::db::checkpoint::s3_store::S3Config {
                        bucket,
                        prefix,
                        region,
                        endpoint: self.s3_endpoint.clone(),
                        path_style: self.s3_path_style,
                    },
                )
                .map_err(|err| CheckpointError::Remote(err.to_string()))?;
                Ok(manager.with_remote_store(std::sync::Arc::new(store)))
            }
        }
    }

    fn apply_region_local_overrides(&mut self) {
        let Some(region) = self.local_region.as_deref() else {
            return;
        };
        if let Some(bucket) = self.s3_bucket_by_region.get(region) {
            self.s3_bucket = Some(bucket.clone());
        }
        if let Some(s3_region) = self.s3_region_by_region.get(region) {
            self.s3_region = Some(s3_region.clone());
        }
        if let Some(endpoint) = self.s3_endpoint_by_region.get(region) {
            self.s3_endpoint = Some(endpoint.clone());
        }
    }
}

fn local_region_from_env() -> Option<String> {
    std::env::var("WRELADB_REGION")
        .ok()
        .or_else(|| std::env::var("FLY_REGION").ok())
        .and_then(|raw| {
            let normalized = raw.trim().to_ascii_lowercase();
            if normalized.is_empty() {
                None
            } else {
                Some(normalized)
            }
        })
}

fn parse_region_string_map_env(key: &str) -> Result<BTreeMap<String, String>, String> {
    let Some(raw) = std::env::var(key).ok() else {
        return Ok(BTreeMap::new());
    };
    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|err| format!("{key} is invalid json object: {err}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| format!("{key} must be a json object"))?;
    let mut map = BTreeMap::new();
    for (region, bucket) in object {
        let normalized_region = region.trim().to_ascii_lowercase();
        if normalized_region.is_empty() {
            continue;
        }
        let Some(bucket_value) = bucket.as_str() else {
            return Err(format!(
                "{key} entry for region `{region}` must be a string value"
            ));
        };
        let trimmed = bucket_value.trim();
        if trimmed.is_empty() {
            continue;
        }
        map.insert(normalized_region, trimmed.to_string());
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvSnapshot {
        key: &'static str,
        value: Option<String>,
    }

    struct EnvGuard {
        snapshots: Vec<EnvSnapshot>,
    }

    impl EnvGuard {
        fn set(vars: &[(&'static str, Option<&str>)]) -> Self {
            let mut snapshots = Vec::with_capacity(vars.len());
            for (key, value) in vars {
                snapshots.push(EnvSnapshot {
                    key,
                    value: std::env::var(key).ok(),
                });
                match value {
                    Some(value) => {
                        // SAFETY: protected by ENV_LOCK for test-local env mutation.
                        unsafe { std::env::set_var(key, value) };
                    }
                    None => {
                        // SAFETY: protected by ENV_LOCK for test-local env mutation.
                        unsafe { std::env::remove_var(key) };
                    }
                }
            }
            Self { snapshots }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for snapshot in &self.snapshots {
                match &snapshot.value {
                    Some(value) => {
                        // SAFETY: protected by ENV_LOCK for test-local env mutation.
                        unsafe { std::env::set_var(snapshot.key, value) };
                    }
                    None => {
                        // SAFETY: protected by ENV_LOCK for test-local env mutation.
                        unsafe { std::env::remove_var(snapshot.key) };
                    }
                }
            }
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "wrela_checkpoint_{}_{}_{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("epoch")
                .as_nanos(),
        ));
        std::fs::create_dir_all(&base).expect("mkdir");
        base
    }

    #[test]
    fn create_and_restore_checkpoint_roundtrip() {
        let data_dir = temp_dir("data");
        std::fs::write(data_dir.join(WAL_FILE), b"wal").expect("wal");
        std::fs::write(data_dir.join(RAFT_FILE), b"raft").expect("raft");
        std::fs::write(data_dir.join(HLC_FILE), b"hlc").expect("hlc");
        std::fs::write(data_dir.join(CDC_FILE), b"cdc").expect("cdc");
        std::fs::write(data_dir.join(SCHEMA_EPOCH_FILE), b"schema").expect("schema");

        let checkpoint_root = temp_dir("ckpt");
        let manager = CheckpointManager::new(&checkpoint_root, 3);
        let info = manager.create_checkpoint(&data_dir).expect("checkpoint");
        assert!(!info.checkpoint_id.is_empty());

        std::fs::remove_file(data_dir.join(WAL_FILE)).expect("remove wal");
        std::fs::remove_file(data_dir.join(RAFT_FILE)).expect("remove raft");

        manager.restore_latest(&data_dir).expect("restore");
        assert_eq!(
            std::fs::read(data_dir.join(WAL_FILE)).expect("wal"),
            b"wal".to_vec()
        );
        assert_eq!(
            std::fs::read(data_dir.join(RAFT_FILE)).expect("raft"),
            b"raft".to_vec()
        );
    }

    #[test]
    fn restore_checkpoint_restores_requested_id() {
        let data_dir = temp_dir("restore_specific_data");
        std::fs::write(data_dir.join(WAL_FILE), b"wal-v1").expect("wal-v1");
        std::fs::write(data_dir.join(RAFT_FILE), b"raft").expect("raft");
        let checkpoint_root = temp_dir("restore_specific_ckpt");
        let manager = CheckpointManager::new(&checkpoint_root, 5);

        let first = manager.create_checkpoint(&data_dir).expect("first");
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(data_dir.join(WAL_FILE), b"wal-v2").expect("wal-v2");
        manager.create_checkpoint(&data_dir).expect("second");

        std::fs::remove_file(data_dir.join(WAL_FILE)).expect("remove wal");
        let restored = manager
            .restore_checkpoint(&data_dir, &first.checkpoint_id)
            .expect("restore by id");
        assert_eq!(restored.checkpoint_id, first.checkpoint_id);
        assert_eq!(
            std::fs::read(data_dir.join(WAL_FILE)).expect("wal"),
            b"wal-v1".to_vec()
        );
    }

    #[test]
    fn prune_keeps_latest_n() {
        let data_dir = temp_dir("data2");
        std::fs::write(data_dir.join(WAL_FILE), b"wal").expect("wal");
        let checkpoint_root = temp_dir("ckpt2");
        let manager = CheckpointManager::new(&checkpoint_root, 1);
        manager.create_checkpoint(&data_dir).expect("c1");
        std::thread::sleep(std::time::Duration::from_millis(1100));
        manager.create_checkpoint(&data_dir).expect("c2");
        let list = manager.list_checkpoints().expect("list");
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn from_env_prefers_region_local_bucket_mapping() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        let _env = EnvGuard::set(&[
            ("WRELADB_CHECKPOINT_BACKEND", Some("s3")),
            ("WRELADB_REGION", Some("ord")),
            ("FLY_REGION", Some("iad")),
            ("WRELADB_S3_BUCKET", Some("fallback-bucket")),
            ("WRELADB_S3_REGION", Some("us-east-1")),
            (
                "WRELADB_S3_BUCKET_BY_REGION_JSON",
                Some("{\"ord\":\"ord-bucket\",\"iad\":\"iad-bucket\"}"),
            ),
            (
                "WRELADB_S3_REGION_BY_REGION_JSON",
                Some("{\"ord\":\"auto\",\"iad\":\"auto\"}"),
            ),
            (
                "WRELADB_S3_ENDPOINT_BY_REGION_JSON",
                Some("{\"ord\":\"https://ord.storage.tigris.dev\"}"),
            ),
        ]);
        let cfg = CheckpointConfig::from_env();
        assert_eq!(cfg.local_region.as_deref(), Some("ord"));
        assert_eq!(cfg.s3_bucket.as_deref(), Some("ord-bucket"));
        assert_eq!(cfg.s3_region.as_deref(), Some("auto"));
        assert_eq!(
            cfg.s3_endpoint.as_deref(),
            Some("https://ord.storage.tigris.dev")
        );
        assert!(cfg.env_parse_error.is_none());
    }

    #[test]
    fn from_env_uses_fly_region_when_wreladb_region_missing() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        let _env = EnvGuard::set(&[
            ("WRELADB_CHECKPOINT_BACKEND", Some("s3")),
            ("WRELADB_REGION", None),
            ("FLY_REGION", Some("iad")),
            ("WRELADB_S3_BUCKET", Some("fallback-bucket")),
            ("WRELADB_S3_REGION", Some("us-east-1")),
            (
                "WRELADB_S3_BUCKET_BY_REGION_JSON",
                Some("{\"iad\":\"iad-bucket\"}"),
            ),
            (
                "WRELADB_S3_REGION_BY_REGION_JSON",
                Some("{\"iad\":\"auto\"}"),
            ),
        ]);
        let cfg = CheckpointConfig::from_env();
        assert_eq!(cfg.local_region.as_deref(), Some("iad"));
        assert_eq!(cfg.s3_bucket.as_deref(), Some("iad-bucket"));
        assert_eq!(cfg.s3_region.as_deref(), Some("auto"));
    }

    #[test]
    fn build_manager_fails_closed_on_invalid_region_map_json() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        let _env = EnvGuard::set(&[
            ("WRELADB_CHECKPOINT_BACKEND", Some("s3")),
            ("WRELADB_REGION", Some("ord")),
            ("WRELADB_S3_BUCKET", Some("fallback-bucket")),
            ("WRELADB_S3_REGION", Some("us-east-1")),
            ("WRELADB_S3_BUCKET_BY_REGION_JSON", Some("{invalid-json")),
        ]);
        let cfg = CheckpointConfig::from_env();
        let err = match cfg.build_manager() {
            Ok(_) => panic!("must fail closed"),
            Err(err) => err,
        };
        assert!(
            err.to_string().contains("invalid checkpoint env config"),
            "unexpected error: {err}"
        );
    }

    /// In-memory checkpoint store for testing reconciliation and crash scenarios.
    struct InMemoryRemoteStore {
        objects: Mutex<HashMap<String, Vec<u8>>>,
    }

    impl InMemoryRemoteStore {
        fn new() -> Self {
            Self {
                objects: Mutex::new(HashMap::new()),
            }
        }
    }

    impl CheckpointStore for InMemoryRemoteStore {
        type Error = crate::db::checkpoint::s3_store::S3CheckpointError;

        fn put_object(&self, key: &str, data: &[u8]) -> Result<(), Self::Error> {
            self.objects
                .lock()
                .expect("lock")
                .insert(key.to_string(), data.to_vec());
            Ok(())
        }

        fn get_object(&self, key: &str) -> Result<Vec<u8>, Self::Error> {
            self.objects
                .lock()
                .expect("lock")
                .get(key)
                .cloned()
                .ok_or_else(|| {
                    crate::db::checkpoint::s3_store::S3CheckpointError::MissingObject(
                        key.to_string(),
                    )
                })
        }

        fn list_prefix(&self, prefix: &str) -> Result<Vec<String>, Self::Error> {
            let guard = self.objects.lock().expect("lock");
            Ok(guard
                .keys()
                .filter(|k| k.starts_with(prefix))
                .cloned()
                .collect())
        }

        fn delete_object(&self, key: &str) -> Result<(), Self::Error> {
            self.objects.lock().expect("lock").remove(key);
            Ok(())
        }

        fn exists(&self, key: &str) -> Result<bool, Self::Error> {
            Ok(self.objects.lock().expect("lock").contains_key(key))
        }
    }

    #[test]
    fn checkpoint_divergent_latest_pointers_reconciled_on_restore() {
        let data_dir = temp_dir("reconcile_data");
        std::fs::write(data_dir.join(WAL_FILE), b"wal-r").expect("wal");

        let checkpoint_root = temp_dir("reconcile_ckpt");
        let remote = std::sync::Arc::new(InMemoryRemoteStore::new());
        let manager = CheckpointManager::new(&checkpoint_root, 3).with_remote_store(remote.clone());

        // Create first checkpoint.
        let first = manager.create_checkpoint(&data_dir).expect("c1");
        std::thread::sleep(std::time::Duration::from_millis(1100));

        // Create second checkpoint.
        std::fs::write(data_dir.join(WAL_FILE), b"wal-r2").expect("wal2");
        let second = manager.create_checkpoint(&data_dir).expect("c2");

        // Simulate crash: revert local LATEST pointer to first checkpoint.
        manager
            .local_store
            .put_object(LATEST_POINTER_FILE, first.checkpoint_id.as_bytes())
            .expect("revert local");

        // Remote still points to second — restore should reconcile.
        std::fs::remove_file(data_dir.join(WAL_FILE)).ok();
        let restored = manager.restore_latest(&data_dir).expect("restore");
        assert_eq!(restored.checkpoint_id, second.checkpoint_id);
    }

    #[test]
    fn checkpoint_crash_during_artifact_write_falls_back_cleanly() {
        let data_dir = temp_dir("crash_data");
        std::fs::write(data_dir.join(WAL_FILE), b"wal-c").expect("wal");
        std::fs::write(data_dir.join(RAFT_FILE), b"raft-c").expect("raft");

        let checkpoint_root = temp_dir("crash_ckpt");
        let manager = CheckpointManager::new(&checkpoint_root, 5);

        // Create a valid checkpoint first.
        manager.create_checkpoint(&data_dir).expect("c1");

        // Simulate a partial checkpoint: write a manifest but corrupt an artifact.
        let bad_id = "9999999-bad";
        let manifest = CheckpointManifest {
            version: CHECKPOINT_MANIFEST_VERSION,
            checkpoint_id: bad_id.to_string(),
            created_at_epoch_s: u64::MAX - 1,
            artifacts: vec![CheckpointArtifact {
                name: WAL_FILE.to_string(),
                checksum: 0xDEAD,
                size_bytes: 5,
            }],
        };
        let payload = serde_json::to_vec_pretty(&manifest).expect("ser");
        let manifest_key = format!("checkpoints/{bad_id}/{MANIFEST_FILE}");
        manager
            .local_store
            .put_object(&manifest_key, &payload)
            .expect("put manifest");
        manager
            .local_store
            .put_object(LATEST_POINTER_FILE, bad_id.as_bytes())
            .expect("set LATEST");
        // Intentionally do NOT write the WAL artifact — simulates crash.

        std::fs::remove_file(data_dir.join(WAL_FILE)).ok();
        std::fs::remove_file(data_dir.join(RAFT_FILE)).ok();
        // restore_latest should skip the corrupt checkpoint and fall back.
        let restored = manager.restore_latest(&data_dir).expect("fallback restore");
        assert_ne!(restored.checkpoint_id, bad_id);
    }

    #[test]
    fn checkpoint_restore_prefers_newer_manifest_when_pointers_diverge() {
        let data_dir = temp_dir("prefer_newer_data");
        std::fs::write(data_dir.join(WAL_FILE), b"wal-pn").expect("wal");

        let checkpoint_root = temp_dir("prefer_newer_ckpt");
        let remote = std::sync::Arc::new(InMemoryRemoteStore::new());
        let manager = CheckpointManager::new(&checkpoint_root, 5).with_remote_store(remote.clone());

        let first = manager.create_checkpoint(&data_dir).expect("c1");
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(data_dir.join(WAL_FILE), b"wal-pn2").expect("wal2");
        let second = manager.create_checkpoint(&data_dir).expect("c2");

        // Set remote to first (older), local to second (newer).
        remote
            .put_object(LATEST_POINTER_FILE, first.checkpoint_id.as_bytes())
            .expect("set remote");

        std::fs::remove_file(data_dir.join(WAL_FILE)).ok();
        let restored = manager.restore_latest(&data_dir).expect("restore");
        // Should prefer the newer (second) checkpoint.
        assert_eq!(restored.checkpoint_id, second.checkpoint_id);
        // After reconciliation, remote LATEST should also point to second.
        let remote_latest = remote
            .get_object(LATEST_POINTER_FILE)
            .expect("remote latest");
        assert_eq!(
            String::from_utf8(remote_latest).expect("utf8").trim(),
            second.checkpoint_id
        );
    }

    #[test]
    fn concurrent_checkpoint_create_and_prune_respects_retention() {
        let data_dir = temp_dir("concurrent_data");
        std::fs::write(data_dir.join(WAL_FILE), b"wal-cc").expect("wal");

        let checkpoint_root = temp_dir("concurrent_ckpt");
        let manager = CheckpointManager::new(&checkpoint_root, 2);

        // Create 3 checkpoints sequentially (retention = 2).
        for i in 0..3 {
            if i > 0 {
                std::thread::sleep(std::time::Duration::from_millis(1100));
            }
            manager.create_checkpoint(&data_dir).expect("create");
        }

        // After creation (which prunes internally), should have at most 2.
        let list = manager.list_checkpoints().expect("list");
        assert!(
            list.len() <= 2,
            "retention must be respected: found {} checkpoints",
            list.len()
        );

        // Explicit prune from a second thread while creating on the first.
        let m2 = manager.clone();
        let d2 = data_dir.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1100));
            m2.create_checkpoint(&d2).expect("concurrent create");
        });
        manager.prune_local().expect("concurrent prune");
        handle.join().expect("thread join");

        let final_list = manager.list_checkpoints().expect("final list");
        assert!(
            final_list.len() <= 2,
            "retention must hold after concurrent ops: found {} checkpoints",
            final_list.len()
        );
    }
}
