use std::collections::HashMap;
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
}

pub fn storage_config() -> StorageConfig {
    let enabled = read_env_bool("WRELA_STORE_ENABLED", false);
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

fn read_env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|val| val.parse::<u64>().ok())
        .unwrap_or(default)
}

pub fn batch_max_delay(config: &StorageConfig) -> Duration {
    Duration::from_millis(config.batch_max_ms.max(1))
}
