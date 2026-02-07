pub async fn storage_set_bytes(_key: &str, _value: &[u8]) -> bool {
    false
}

pub async fn storage_get_bytes(_key: &str) -> Option<Vec<u8>> {
    None
}

pub async fn storage_set_string(_key: &str, _value: &str) -> bool {
    false
}
