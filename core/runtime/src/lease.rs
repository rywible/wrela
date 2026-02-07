pub fn owner_id() -> String {
    "runtime-trimmed".to_string()
}

pub async fn try_acquire_lease(_key: &str, _owner: &str, _ttl_secs: u64) -> bool {
    false
}
