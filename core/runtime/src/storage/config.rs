use std::collections::HashMap;
#[cfg(any(test, feature = "test-utils"))]
use std::future::Future;
use std::sync::OnceLock;
use std::time::Duration;

#[cfg(any(test, feature = "test-utils"))]
tokio::task_local! {
    static STORAGE_CONFIG_OVERRIDE: StorageConfig;
}

#[derive(Clone, Debug)]
pub struct StorageConfig {
    pub enabled: bool,
    pub path: String,
    pub node_id: u64,
    pub bind_addr: String,
    pub http_enabled: bool,
    pub peer_token: Option<String>,
    pub peers: HashMap<u64, String>,
    pub bootstrap: bool,
    pub snapshot_interval: u64,
    pub batch_max_ops: usize,
    pub batch_max_ms: u64,
    pub queue_cap: usize,
    pub blob: BlobConfig,
    pub backup: BackupConfig,
}

#[derive(Clone, Debug)]
pub struct BlobConfig {
    pub threshold_bytes: usize,
    pub file_path: String,
    pub s3: Option<S3Config>,
}

#[derive(Clone, Debug)]
pub struct S3Config {
    pub bucket: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
    pub endpoint: Option<String>,
    pub prefix: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct StorageUserConfig {
    pub enabled: Option<bool>,
    pub file_path: Option<String>,
    pub node_id: Option<u64>,
    pub bind_addr: Option<String>,
    pub http_enabled: Option<bool>,
    pub peer_token: Option<String>,
    pub peers_raw: Option<String>,
    pub peers: Option<HashMap<u64, String>>,
    pub bootstrap: Option<bool>,
    pub snapshot_interval: Option<u64>,
    pub batch_max_ops: Option<usize>,
    pub batch_max_ms: Option<u64>,
    pub queue_cap: Option<usize>,
    pub blob_threshold_bytes: Option<usize>,
    pub blob_path: Option<String>,
    pub backup_enabled: Option<bool>,
    pub backup_max_age_secs: Option<u64>,
    pub backup_max_logs: Option<usize>,
    pub backup_retention_days: Option<u64>,
    pub backup_max_keep: Option<usize>,
    pub backup_prefix: Option<String>,
    pub backup_only_leader: Option<bool>,
    pub backup_restore_mode: Option<String>,
    pub backup_restore_id: Option<String>,
    pub s3_bucket: Option<String>,
    pub s3_region: Option<String>,
    pub s3_access_key: Option<String>,
    pub s3_secret_key: Option<String>,
    pub s3_endpoint: Option<String>,
    pub s3_prefix: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestoreMode {
    None,
    Single,
    Cluster,
}

#[derive(Clone, Debug)]
pub struct BackupConfig {
    pub enabled: bool,
    pub max_age_secs: u64,
    pub max_logs: usize,
    pub retention_days: u64,
    pub max_keep: usize,
    pub prefix: String,
    pub only_leader: bool,
    pub restore_mode: RestoreMode,
    pub restore_id: Option<String>,
}

static STORAGE_USER_CONFIG: OnceLock<StorageUserConfig> = OnceLock::new();

pub fn storage_config() -> StorageConfig {
    #[cfg(any(test, feature = "test-utils"))]
    if let Ok(config) = STORAGE_CONFIG_OVERRIDE.try_with(|cfg| cfg.clone()) {
        return config;
    }
    let user = STORAGE_USER_CONFIG.get();
    let enabled = user
        .and_then(|cfg| cfg.enabled)
        .unwrap_or_else(|| user.is_some());
    let path = user
        .and_then(|cfg| cfg.file_path.clone())
        .unwrap_or_else(|| "./wrela.db".to_string());
    let node_id = user.and_then(|cfg| cfg.node_id).unwrap_or(1);
    let bind_addr = user
        .and_then(|cfg| cfg.bind_addr.clone())
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());
    let http_enabled = user.and_then(|cfg| cfg.http_enabled).unwrap_or(true);
    let peer_token = user.and_then(|cfg| cfg.peer_token.clone()).and_then(|val| {
        if val.trim().is_empty() {
            None
        } else {
            Some(val)
        }
    });
    let peers = user
        .and_then(|cfg| cfg.peers.clone())
        .or_else(|| {
            user.and_then(|cfg| cfg.peers_raw.clone())
                .map(|raw| parse_peers(Some(raw)))
        })
        .unwrap_or_default();
    let bootstrap = user
        .and_then(|cfg| cfg.bootstrap)
        .unwrap_or_else(|| peers.is_empty());
    let snapshot_interval = user.and_then(|cfg| cfg.snapshot_interval).unwrap_or(10_000);
    let batch_max_ops = user.and_then(|cfg| cfg.batch_max_ops).unwrap_or(128).max(1);
    let batch_max_ms = user.and_then(|cfg| cfg.batch_max_ms).unwrap_or(5).max(1);
    let queue_cap = user.and_then(|cfg| cfg.queue_cap).unwrap_or(1024).max(1);
    let blob = blob_config(user);
    let backup = backup_config(user, &blob);
    StorageConfig {
        enabled,
        path,
        node_id,
        bind_addr,
        http_enabled,
        peer_token,
        peers,
        bootstrap,
        snapshot_interval,
        batch_max_ops,
        batch_max_ms,
        queue_cap,
        blob,
        backup,
    }
}

pub fn set_storage_user_config(config: StorageUserConfig) {
    let _ = STORAGE_USER_CONFIG.set(config);
}

#[cfg(any(test, feature = "test-utils"))]
pub async fn with_storage_config_override<F, R>(config: StorageConfig, fut: F) -> R
where
    F: Future<Output = R>,
{
    STORAGE_CONFIG_OVERRIDE.scope(config, fut).await
}

#[cfg(any(test, feature = "test-utils"))]
pub(crate) fn capture_storage_config_override() -> Option<StorageConfig> {
    STORAGE_CONFIG_OVERRIDE.try_with(Clone::clone).ok()
}

#[cfg(any(test, feature = "test-utils"))]
pub(crate) async fn with_storage_config_override_if_present<F, R>(
    config: Option<StorageConfig>,
    fut: F,
) -> R
where
    F: Future<Output = R>,
{
    if let Some(config) = config {
        STORAGE_CONFIG_OVERRIDE.scope(config, fut).await
    } else {
        fut.await
    }
}

fn parse_peers(raw: Option<String>) -> HashMap<u64, String> {
    let mut peers = HashMap::new();
    let Some(raw) = raw else { return peers };
    for entry in raw.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let Some((id, addr)) = entry.split_once('=') else {
            continue;
        };
        if let Ok(id) = id.trim().parse::<u64>() {
            let addr = addr.trim().to_string();
            if !addr.is_empty() {
                peers.insert(id, addr);
            }
        }
    }
    peers
}

fn backup_config(user: Option<&StorageUserConfig>, blob: &BlobConfig) -> BackupConfig {
    let enabled_default = blob.s3.is_some();
    let enabled = user
        .and_then(|cfg| cfg.backup_enabled)
        .unwrap_or(enabled_default);
    let max_age_secs = user
        .and_then(|cfg| cfg.backup_max_age_secs)
        .unwrap_or(3600)
        .max(60);
    let max_logs = user
        .and_then(|cfg| cfg.backup_max_logs)
        .unwrap_or(100_000)
        .max(1);
    let retention_days = user.and_then(|cfg| cfg.backup_retention_days).unwrap_or(7);
    let max_keep = user.and_then(|cfg| cfg.backup_max_keep).unwrap_or(0);
    let prefix = user
        .and_then(|cfg| cfg.backup_prefix.clone())
        .unwrap_or_else(|| "backups".to_string());
    let only_leader = user.and_then(|cfg| cfg.backup_only_leader).unwrap_or(true);
    let restore_mode = match user
        .and_then(|cfg| cfg.backup_restore_mode.as_deref())
        .unwrap_or("single")
    {
        "cluster" => RestoreMode::Cluster,
        "single" => RestoreMode::Single,
        "none" => RestoreMode::None,
        _ => RestoreMode::Single,
    };
    let restore_id = user.and_then(|cfg| cfg.backup_restore_id.clone());
    BackupConfig {
        enabled,
        max_age_secs,
        max_logs,
        retention_days,
        max_keep,
        prefix,
        only_leader,
        restore_mode,
        restore_id,
    }
}

pub fn batch_max_delay(config: &StorageConfig) -> Duration {
    Duration::from_millis(config.batch_max_ms.max(1))
}

fn blob_config(user: Option<&StorageUserConfig>) -> BlobConfig {
    let file_path = user
        .and_then(|cfg| cfg.blob_path.clone())
        .or_else(|| user.and_then(|cfg| cfg.file_path.clone()))
        .unwrap_or_else(|| "./wrela.blobs".to_string());
    let s3 = s3_config(user);
    let threshold_bytes = user
        .and_then(|cfg| cfg.blob_threshold_bytes)
        .unwrap_or(256 * 1024);
    BlobConfig {
        threshold_bytes,
        file_path,
        s3,
    }
}

fn s3_config(user: Option<&StorageUserConfig>) -> Option<S3Config> {
    let bucket = user.and_then(|cfg| cfg.s3_bucket.clone());
    let region = user.and_then(|cfg| cfg.s3_region.clone());
    let access_key = user.and_then(|cfg| cfg.s3_access_key.clone());
    let secret_key = user.and_then(|cfg| cfg.s3_secret_key.clone());
    let endpoint = user.and_then(|cfg| cfg.s3_endpoint.clone());
    let prefix = user.and_then(|cfg| cfg.s3_prefix.clone());

    let Some(bucket) = bucket else { return None };
    let Some(region) = region else { return None };
    let Some(access_key) = access_key else {
        return None;
    };
    let Some(secret_key) = secret_key else {
        return None;
    };

    Some(S3Config {
        bucket,
        region,
        access_key,
        secret_key,
        endpoint,
        prefix,
    })
}
