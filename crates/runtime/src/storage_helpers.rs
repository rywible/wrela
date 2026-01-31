use crate::storage::service::{StorageRequest, StorageResponse, StorageService};
use crate::string;
use crate::value::Value;
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

pub async fn storage_delete(key: &str) -> bool {
    StorageService::dispatch(StorageRequest::Delete {
        key: key.as_bytes().to_vec(),
    })
    .await
    .is_ok()
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

pub fn value_to_string(val: Value) -> Option<String> {
    string::with_string_bytes(val, |bytes| String::from_utf8_lossy(bytes).into_owned())
}

pub async fn storage_get_json_vec<T>(key: &str) -> Vec<T>
where
    T: serde::de::DeserializeOwned,
{
    storage_get_json::<Vec<T>>(key).await.unwrap_or_default()
}
