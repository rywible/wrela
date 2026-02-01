use crate::storage::config::storage_config;
use crate::storage_helpers::{
    storage_delete_if_version, storage_get_string_with_version, storage_set_string_if_version,
};
use serde::{Deserialize, Serialize};
#[cfg(not(any(test, feature = "test-utils")))]
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Deserialize, Serialize)]
struct LeaseRecord {
    owner: String,
    exp: u64,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn owner_id() -> String {
    #[cfg(any(test, feature = "test-utils"))]
    {
        let node_id = storage_config().node_id;
        return format!("node-{node_id}-{}", std::process::id());
    }
    #[cfg(not(any(test, feature = "test-utils")))]
    {
        static OWNER: OnceLock<String> = OnceLock::new();
        OWNER
            .get_or_init(|| {
                let node_id = storage_config().node_id;
                format!("node-{node_id}-{}", std::process::id())
            })
            .clone()
    }
}

fn encode_lease(owner: &str, exp: u64) -> Option<String> {
    serde_json::to_string(&LeaseRecord {
        owner: owner.to_string(),
        exp,
    })
    .ok()
}

fn parse_lease(raw: &str) -> Option<LeaseRecord> {
    serde_json::from_str(raw).ok()
}

pub async fn try_acquire_lease(key: &str, owner: &str, ttl_secs: u64) -> bool {
    let now = now_secs();
    let exp = now.saturating_add(ttl_secs.max(1));
    let Some(encoded) = encode_lease(owner, exp) else {
        return false;
    };
    match storage_get_string_with_version(key).await {
        Some((raw, version)) => {
            if let Some(existing) = parse_lease(&raw) {
                if existing.exp > now && existing.owner != owner {
                    return false;
                }
            }
            storage_set_string_if_version(key, &encoded, Some(version)).await
        }
        None => storage_set_string_if_version(key, &encoded, None).await,
    }
}

pub async fn renew_lease(key: &str, owner: &str, ttl_secs: u64) -> bool {
    let now = now_secs();
    let exp = now.saturating_add(ttl_secs.max(1));
    let Some(encoded) = encode_lease(owner, exp) else {
        return false;
    };
    let Some((raw, version)) = storage_get_string_with_version(key).await else {
        return false;
    };
    let Some(existing) = parse_lease(&raw) else {
        return false;
    };
    if existing.owner != owner || existing.exp <= now {
        return false;
    }
    storage_set_string_if_version(key, &encoded, Some(version)).await
}

pub async fn release_lease(key: &str, owner: &str) -> bool {
    let Some((raw, version)) = storage_get_string_with_version(key).await else {
        return false;
    };
    let Some(existing) = parse_lease(&raw) else {
        return false;
    };
    if existing.owner != owner {
        return false;
    }
    storage_delete_if_version(key, Some(version)).await
}
