use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct StorageConfig {
    pub enabled: bool,
    pub path: String,
    pub node_id: u64,
    pub bind_addr: String,
    pub http_enabled: bool,
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
    pub file_path: Option<String>,
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
    let user = STORAGE_USER_CONFIG.get();
    let enabled = user.is_some() || read_env_bool("WRELA_STORE_ENABLED", false);
    let path = std::env::var("WRELA_STORE_PATH").unwrap_or_else(|_| "./wrela.db".to_string());
    let node_id = read_env_u64("WRELA_RAFT_NODE_ID", 1);
    let bind_addr =
        std::env::var("WRELA_RAFT_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    let http_enabled = read_env_bool("WRELA_RAFT_HTTP_ENABLED", true);
    let peers = parse_peers(std::env::var("WRELA_RAFT_PEERS").ok());
    let bootstrap = read_env_bool("WRELA_RAFT_BOOTSTRAP", peers.is_empty());
    let snapshot_interval = read_env_u64("WRELA_RAFT_SNAPSHOT_INTERVAL", 10_000);
    let batch_max_ops = read_env_usize("WRELA_STORE_BATCH_MAX_OPS", 128).max(1);
    let batch_max_ms = read_env_u64("WRELA_STORE_BATCH_MAX_MS", 5).max(1);
    let queue_cap = read_env_usize("WRELA_STORE_QUEUE_CAP", 1024).max(1);
    let blob = blob_config(user);
    let backup = backup_config(&blob);
    StorageConfig {
        enabled,
        path,
        node_id,
        bind_addr,
        http_enabled,
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

fn read_env_bool(key: &str, default: bool) -> bool {
    std::env::var(key)
        .ok()
        .map(|v| !matches!(v.as_str(), "0" | "false" | "off"))
        .unwrap_or(default)
}

fn read_env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|val| val.parse::<usize>().ok())
        .unwrap_or(default)
}

fn read_env_string(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn backup_config(blob: &BlobConfig) -> BackupConfig {
    let enabled_default = blob.s3.is_some();
    let enabled = std::env::var("WRELA_BACKUP_ENABLED")
        .ok()
        .map(|v| !matches!(v.as_str(), "0" | "false" | "off"))
        .unwrap_or(enabled_default);
    let max_age_secs = read_env_u64("WRELA_BACKUP_MAX_AGE_SECS", 3600).max(60);
    let max_logs = read_env_usize("WRELA_BACKUP_MAX_LOGS", 100_000).max(1);
    let retention_days = read_env_u64("WRELA_BACKUP_RETENTION_DAYS", 7);
    let max_keep = read_env_usize("WRELA_BACKUP_MAX_KEEP", 0);
    let prefix = read_env_string("WRELA_BACKUP_PREFIX", "backups");
    let only_leader = read_env_bool("WRELA_BACKUP_ONLY_LEADER", true);
    let restore_mode = match std::env::var("WRELA_BACKUP_RESTORE_MODE").ok().as_deref() {
        Some("cluster") => RestoreMode::Cluster,
        Some("single") => RestoreMode::Single,
        Some("none") => RestoreMode::None,
        _ => RestoreMode::Single,
    };
    let restore_id = std::env::var("WRELA_BACKUP_RESTORE_ID").ok();
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

fn read_env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|val| val.parse::<u64>().ok())
        .unwrap_or(default)
}

pub fn batch_max_delay(config: &StorageConfig) -> Duration {
    Duration::from_millis(config.batch_max_ms.max(1))
}

fn blob_config(user: Option<&StorageUserConfig>) -> BlobConfig {
    let use_env = user.is_none();
    let file_path = user
        .and_then(|cfg| cfg.file_path.clone())
        .or_else(|| {
            if use_env {
                std::env::var("WRELA_STORE_BLOB_PATH").ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| "./wrela.blobs".to_string());
    let s3 = s3_config(user);
    BlobConfig {
        threshold_bytes: 256 * 1024,
        file_path,
        s3,
    }
}

fn s3_config(user: Option<&StorageUserConfig>) -> Option<S3Config> {
    let use_env = user.is_none();
    let bucket = user.and_then(|cfg| cfg.s3_bucket.clone()).or_else(|| {
        if use_env {
            std::env::var("WRELA_STORE_BLOB_S3_BUCKET").ok()
        } else {
            None
        }
    });
    let region = user.and_then(|cfg| cfg.s3_region.clone()).or_else(|| {
        if use_env {
            std::env::var("WRELA_STORE_BLOB_S3_REGION").ok()
        } else {
            None
        }
    });
    let access_key = user.and_then(|cfg| cfg.s3_access_key.clone()).or_else(|| {
        if use_env {
            std::env::var("WRELA_STORE_BLOB_S3_ACCESS_KEY").ok()
        } else {
            None
        }
    });
    let secret_key = user.and_then(|cfg| cfg.s3_secret_key.clone()).or_else(|| {
        if use_env {
            std::env::var("WRELA_STORE_BLOB_S3_SECRET_KEY").ok()
        } else {
            None
        }
    });
    let endpoint = user.and_then(|cfg| cfg.s3_endpoint.clone()).or_else(|| {
        if use_env {
            std::env::var("WRELA_STORE_BLOB_S3_ENDPOINT").ok()
        } else {
            None
        }
    });
    let prefix = user.and_then(|cfg| cfg.s3_prefix.clone()).or_else(|| {
        if use_env {
            std::env::var("WRELA_STORE_BLOB_S3_PREFIX").ok()
        } else {
            None
        }
    });

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
