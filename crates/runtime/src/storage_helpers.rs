use crate::map;
use crate::storage::service::{StorageRequest, StorageResponse, StorageService};
use crate::storage::service::StorageError;
use crate::string;
use crate::value::{Value, int_value};
use crate::wr_rc_dec;

pub async fn storage_get_string(key: &str) -> Option<String> {
    let resp = StorageService::dispatch(StorageRequest::Get {
        key: key.as_bytes().to_vec(),
    })
    .await
    .ok()?;
    let value = match resp {
        StorageResponse::Ok(val) => val,
        StorageResponse::Err(_) => return None,
    };
    if value.is_nil() {
        return None;
    }
    let out = string::with_string_bytes(value, |bytes| String::from_utf8_lossy(bytes).into_owned());
    if value.is_ptr() {
        unsafe { wr_rc_dec(value) };
    }
    out
}

pub async fn storage_set_string(key: &str, value: &str) -> bool {
    StorageService::dispatch(StorageRequest::Put {
        key: key.as_bytes().to_vec(),
        value: value.as_bytes().to_vec(),
    })
    .await
    .is_ok()
}

pub async fn storage_set_bytes(key: &str, value: &[u8]) -> bool {
    StorageService::dispatch(StorageRequest::Put {
        key: key.as_bytes().to_vec(),
        value: value.to_vec(),
    })
    .await
    .is_ok()
}

pub async fn storage_delete(key: &str) -> bool {
    StorageService::dispatch(StorageRequest::Delete {
        key: key.as_bytes().to_vec(),
    })
    .await
    .is_ok()
}

fn map_get_string(map_val: Value, key: &str) -> Option<String> {
    let key_val = string::str_from_bytes(key.as_bytes());
    let got = map::map_get(map_val, key_val);
    unsafe { wr_rc_dec(key_val) };
    if got.is_nil() {
        return None;
    }
    let out = string::with_string_bytes(got, |bytes| String::from_utf8_lossy(bytes).into_owned());
    if got.is_ptr() {
        unsafe { wr_rc_dec(got) };
    }
    out
}

fn map_get_u64(map_val: Value, key: &str) -> Option<u64> {
    let key_val = string::str_from_bytes(key.as_bytes());
    let got = map::map_get(map_val, key_val);
    unsafe { wr_rc_dec(key_val) };
    let out = int_value(got).and_then(|v| if v >= 0 { Some(v as u64) } else { None });
    if got.is_ptr() {
        unsafe { wr_rc_dec(got) };
    }
    out
}

pub async fn storage_get_string_with_version(key: &str) -> Option<(String, u64)> {
    let resp = StorageService::dispatch(StorageRequest::GetWithVersion {
        key: key.as_bytes().to_vec(),
    })
    .await
    .ok()?;
    let value = match resp {
        StorageResponse::Ok(val) => val,
        StorageResponse::Err(_) => return None,
    };
    if value.is_nil() {
        return None;
    }
    let out = match (
        map_get_string(value, "value"),
        map_get_u64(value, "version"),
    ) {
        (Some(raw), Some(version)) => Some((raw, version)),
        _ => None,
    };
    if value.is_ptr() {
        unsafe { wr_rc_dec(value) };
    }
    out
}

pub async fn storage_set_string_if_version(
    key: &str,
    value: &str,
    expected_version: Option<u64>,
) -> bool {
    let resp = StorageService::dispatch(StorageRequest::CompareAndSet {
        key: key.as_bytes().to_vec(),
        expected_version,
        value: Some(value.as_bytes().to_vec()),
    })
    .await;
    let Ok(resp) = resp else { return false };
    let value = match resp {
        StorageResponse::Ok(val) => val,
        StorageResponse::Err(_) => return false,
    };
    let applied = value.is_bool() && value.as_bool();
    if value.is_ptr() {
        unsafe { wr_rc_dec(value) };
    }
    applied
}

pub async fn storage_delete_if_version(key: &str, expected_version: Option<u64>) -> bool {
    let resp = StorageService::dispatch(StorageRequest::CompareAndSet {
        key: key.as_bytes().to_vec(),
        expected_version,
        value: None,
    })
    .await;
    let Ok(resp) = resp else { return false };
    let value = match resp {
        StorageResponse::Ok(val) => val,
        StorageResponse::Err(_) => return false,
    };
    let applied = value.is_bool() && value.as_bool();
    if value.is_ptr() {
        unsafe { wr_rc_dec(value) };
    }
    applied
}

pub async fn storage_list_prefix_keys(prefix: &str, limit: usize) -> Vec<String> {
    let resp = StorageService::dispatch(StorageRequest::ListPrefix {
        prefix: prefix.as_bytes().to_vec(),
        limit,
    })
    .await;
    let Ok(resp) = resp else { return Vec::new() };
    let value = match resp {
        StorageResponse::Ok(val) => val,
        StorageResponse::Err(_) => return Vec::new(),
    };
    if value.is_nil() {
        return Vec::new();
    }
    let list_ptr = match crate::list::as_list_ref(value) {
        Some(list) => list,
        None => {
            if value.is_ptr() {
                unsafe { wr_rc_dec(value) };
            }
            return Vec::new();
        }
    };
    let mut out = Vec::new();
    unsafe {
        let list_ref = &(*list_ptr).data;
        for entry in list_ref.iter().take((*list_ptr).len) {
            let entry_val = *entry;
            if let Some(s) = string::with_string_bytes(entry_val, |bytes| {
                String::from_utf8_lossy(bytes).into_owned()
            }) {
                out.push(s);
            }
        }
    }
    if value.is_ptr() {
        unsafe { wr_rc_dec(value) };
    }
    out
}

pub async fn storage_get_json<T>(key: &str) -> Option<T>
where
    T: serde::de::DeserializeOwned,
{
    let raw = storage_get_string(key).await?;
    serde_json::from_str(&raw).ok()
}

pub async fn storage_set_json<T>(key: &str, value: &T) -> bool
where
    T: serde::Serialize + ?Sized,
{
    let Ok(raw) = serde_json::to_string(value) else {
        return false;
    };
    storage_set_string(key, &raw).await
}

pub async fn storage_get_json_result<T>(key: &str) -> Result<Option<T>, StorageError>
where
    T: serde::de::DeserializeOwned,
{
    let resp = StorageService::dispatch(StorageRequest::Get {
        key: key.as_bytes().to_vec(),
    })
    .await?;
    match resp {
        StorageResponse::Ok(val) => {
            if val.is_nil() {
                return Ok(None);
            }
            let raw = string::with_string_bytes(val, |bytes| {
                String::from_utf8_lossy(bytes).into_owned()
            });
            if val.is_ptr() {
                unsafe { wr_rc_dec(val) };
            }
            let Some(raw) = raw else {
                return Ok(None);
            };
            Ok(serde_json::from_str(&raw).ok())
        }
        StorageResponse::Err(err) => Err(StorageError::Internal(err)),
    }
}

pub async fn storage_set_json_result<T>(key: &str, value: &T) -> Result<(), StorageError>
where
    T: serde::Serialize + ?Sized,
{
    let Ok(raw) = serde_json::to_string(value) else {
        return Err(StorageError::Internal("json encode failed".to_string()));
    };
    let resp = StorageService::dispatch(StorageRequest::Put {
        key: key.as_bytes().to_vec(),
        value: raw.as_bytes().to_vec(),
    })
    .await?;
    match resp {
        StorageResponse::Ok(val) => {
            if val.is_ptr() {
                unsafe { wr_rc_dec(val) };
            }
            Ok(())
        }
        StorageResponse::Err(err) => Err(StorageError::Internal(err)),
    }
}

pub fn value_to_string(val: Value) -> Option<String> {
    string::with_string_bytes(val, |bytes| String::from_utf8_lossy(bytes).into_owned())
}

pub async fn storage_get_json_vec<T>(key: &str) -> Vec<T>
where
    T: serde::de::DeserializeOwned,
{
    storage_get_json::<Vec<T>>(key).await.unwrap_or_default()
}

pub async fn storage_get_json_vec_result<T>(key: &str) -> Result<Vec<T>, StorageError>
where
    T: serde::de::DeserializeOwned,
{
    match storage_get_json_result::<Vec<T>>(key).await? {
        Some(values) => Ok(values),
        None => Ok(Vec::new()),
    }
}
