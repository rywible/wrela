mod backup;
pub mod blob;
pub mod config;
pub mod service;
pub mod store;
mod transport;
pub mod value;

#[cfg(test)]
mod backup_tests;
#[cfg(test)]
mod tests;

use crate::actor::{pending_new, resolve_pending, runtime_spawn};
use crate::bytes;
use crate::class;
use crate::list;
use crate::map;
use crate::result;
use crate::string;
use crate::value::{Value, int_value};
use crate::wr_rc_dec;
use config::StorageUserConfig;
use service::{StorageError, StorageRequest, StorageResponse, StorageService};

#[cfg(any(test, feature = "test-utils"))]
pub use transport::set_drop_replication;

fn error_value(message: &str) -> Value {
    let s = string::str_from_utf8(message.as_ptr(), message.len());
    result::result_err(s)
}

fn ok_value(value: Value) -> Value {
    result::result_ok(value)
}

fn key_bytes(val: Value) -> Result<Vec<u8>, Value> {
    string::with_string_bytes(val, |bytes| bytes.to_vec())
        .ok_or_else(|| error_value("storage expects a String key"))
}

fn value_bytes(val: Value) -> Result<Vec<u8>, Value> {
    string::with_string_bytes(val, |bytes| bytes.to_vec())
        .ok_or_else(|| error_value("storage expects a String value"))
}

fn optional_key_bytes(val: Value) -> Result<Option<Vec<u8>>, Value> {
    if val.is_nil() {
        return Ok(None);
    }
    string::with_string_bytes(val, |bytes| bytes.to_vec())
        .map(Some)
        .ok_or_else(|| error_value("storage expects a String key"))
}

fn resolve_response(resp: StorageResponse) -> Value {
    match resp {
        StorageResponse::Ok(value) => ok_value(value),
        StorageResponse::Bytes(Some(bytes)) => ok_value(bytes::bytes_from_slice(&bytes)),
        StorageResponse::Bytes(None) => ok_value(Value::nil()),
        StorageResponse::Err(message) => error_value(&message),
    }
}

fn resolve_error(err: StorageError) -> Value {
    error_value(&err.to_string())
}

pub fn storage_get(key: Value) -> Value {
    let (pending, state) = pending_new();
    let key = match key_bytes(key) {
        Ok(bytes) => bytes,
        Err(err_val) => {
            resolve_pending(state, err_val);
            return pending;
        }
    };
    runtime_spawn(async move {
        let result = StorageService::dispatch(StorageRequest::Get { key }).await;
        let resolved = match result {
            Ok(resp) => resolve_response(resp),
            Err(err) => resolve_error(err),
        };
        resolve_pending(state, resolved);
    });
    pending
}

pub fn storage_get_with_version(key: Value) -> Value {
    let (pending, state) = pending_new();
    let key = match key_bytes(key) {
        Ok(bytes) => bytes,
        Err(err_val) => {
            resolve_pending(state, err_val);
            return pending;
        }
    };
    runtime_spawn(async move {
        let result = StorageService::dispatch(StorageRequest::GetWithVersion { key }).await;
        let resolved = match result {
            Ok(resp) => resolve_response(resp),
            Err(err) => resolve_error(err),
        };
        resolve_pending(state, resolved);
    });
    pending
}

pub fn storage_scan(start: Value, end: Value, limit: Value) -> Value {
    let (pending, state) = pending_new();
    let start = match optional_key_bytes(start) {
        Ok(bytes) => bytes,
        Err(err_val) => {
            resolve_pending(state, err_val);
            return pending;
        }
    };
    let end = match optional_key_bytes(end) {
        Ok(bytes) => bytes,
        Err(err_val) => {
            resolve_pending(state, err_val);
            return pending;
        }
    };
    let limit = int_value(limit).unwrap_or(1000).max(0) as usize;
    runtime_spawn(async move {
        let result = StorageService::dispatch(StorageRequest::Scan { start, end, limit }).await;
        let resolved = match result {
            Ok(resp) => resolve_response(resp),
            Err(err) => resolve_error(err),
        };
        resolve_pending(state, resolved);
    });
    pending
}

pub fn storage_list_prefix(prefix: Value, limit: Value) -> Value {
    let (pending, state) = pending_new();
    let prefix = match key_bytes(prefix) {
        Ok(bytes) => bytes,
        Err(err_val) => {
            resolve_pending(state, err_val);
            return pending;
        }
    };
    let limit = int_value(limit).unwrap_or(1000).max(0) as usize;
    runtime_spawn(async move {
        let result = StorageService::dispatch(StorageRequest::ListPrefix { prefix, limit }).await;
        let resolved = match result {
            Ok(resp) => resolve_response(resp),
            Err(err) => resolve_error(err),
        };
        resolve_pending(state, resolved);
    });
    pending
}

pub fn storage_set(key: Value, value: Value) -> Value {
    let (pending, state) = pending_new();
    let key = match key_bytes(key) {
        Ok(bytes) => bytes,
        Err(err_val) => {
            resolve_pending(state, err_val);
            return pending;
        }
    };
    let value = match value_bytes(value) {
        Ok(bytes) => bytes,
        Err(err_val) => {
            resolve_pending(state, err_val);
            return pending;
        }
    };
    runtime_spawn(async move {
        let result = StorageService::dispatch(StorageRequest::Put { key, value }).await;
        let resolved = match result {
            Ok(resp) => resolve_response(resp),
            Err(err) => resolve_error(err),
        };
        resolve_pending(state, resolved);
    });
    pending
}

pub fn storage_set_if_version(key: Value, value: Value, version: Value) -> Value {
    let (pending, state) = pending_new();
    let key = match key_bytes(key) {
        Ok(bytes) => bytes,
        Err(err_val) => {
            resolve_pending(state, err_val);
            return pending;
        }
    };
    let value = match value_bytes(value) {
        Ok(bytes) => bytes,
        Err(err_val) => {
            resolve_pending(state, err_val);
            return pending;
        }
    };
    let expected = if version.is_nil() {
        None
    } else {
        int_value(version).map(|v| v.max(0) as u64)
    };
    runtime_spawn(async move {
        let result = StorageService::dispatch(StorageRequest::CompareAndSet {
            key,
            expected_version: expected,
            value: Some(value),
        })
        .await;
        let resolved = match result {
            Ok(resp) => resolve_response(resp),
            Err(err) => resolve_error(err),
        };
        resolve_pending(state, resolved);
    });
    pending
}

pub fn storage_delete_if_version(key: Value, version: Value) -> Value {
    let (pending, state) = pending_new();
    let key = match key_bytes(key) {
        Ok(bytes) => bytes,
        Err(err_val) => {
            resolve_pending(state, err_val);
            return pending;
        }
    };
    let expected = if version.is_nil() {
        None
    } else {
        int_value(version).map(|v| v.max(0) as u64)
    };
    runtime_spawn(async move {
        let result = StorageService::dispatch(StorageRequest::CompareAndSet {
            key,
            expected_version: expected,
            value: None,
        })
        .await;
        let resolved = match result {
            Ok(resp) => resolve_response(resp),
            Err(err) => resolve_error(err),
        };
        resolve_pending(state, resolved);
    });
    pending
}

pub fn storage_delete(key: Value) -> Value {
    let (pending, state) = pending_new();
    let key = match key_bytes(key) {
        Ok(bytes) => bytes,
        Err(err_val) => {
            resolve_pending(state, err_val);
            return pending;
        }
    };
    runtime_spawn(async move {
        let result = StorageService::dispatch(StorageRequest::Delete { key }).await;
        let resolved = match result {
            Ok(resp) => resolve_response(resp),
            Err(err) => resolve_error(err),
        };
        resolve_pending(state, resolved);
    });
    pending
}

fn map_get_string(map_val: Value, key: &str) -> Option<Vec<u8>> {
    let key_val = string::str_from_bytes(key.as_bytes());
    let got = map::map_get(map_val, key_val);
    unsafe { wr_rc_dec(key_val) };
    if got.is_nil() {
        return None;
    }
    let out = string::with_string_bytes(got, |bytes| bytes.to_vec());
    unsafe { wr_rc_dec(got) };
    out
}

pub fn storage_batch_set(items: Value) -> Value {
    let (pending, state) = pending_new();
    let list_ptr = match list::as_list_ref(items) {
        Some(list) => list,
        None => {
            resolve_pending(state, error_value("storage expects a List"));
            return pending;
        }
    };
    let mut out = Vec::new();
    unsafe {
        let list_ref = &(*list_ptr).data;
        for entry in list_ref.iter().take((*list_ptr).len) {
            let map_val = *entry;
            if map::as_map_ref(map_val).is_none() {
                resolve_pending(state, error_value("storage batch expects Map entries"));
                return pending;
            }
            let Some(key) = map_get_string(map_val, "key") else {
                resolve_pending(state, error_value("storage batch requires key"));
                return pending;
            };
            let Some(value) = map_get_string(map_val, "value") else {
                resolve_pending(state, error_value("storage batch requires value"));
                return pending;
            };
            out.push((key, value));
        }
    }
    runtime_spawn(async move {
        let result = StorageService::dispatch(StorageRequest::BatchSet { items: out }).await;
        let resolved = match result {
            Ok(resp) => resolve_response(resp),
            Err(err) => resolve_error(err),
        };
        resolve_pending(state, resolved);
    });
    pending
}

pub fn storage_configure(config: Value) -> Value {
    let user = StorageUserConfig {
        enabled: config_field_bool(config, "enabled"),
        file_path: config_field_string(config, "file_path"),
        node_id: config_field_u64(config, "node_id"),
        bind_addr: config_field_string(config, "bind_addr"),
        http_enabled: config_field_bool(config, "http_enabled"),
        peer_token: config_field_string(config, "peer_token"),
        peers_raw: config_field_string(config, "peers"),
        peers: None,
        bootstrap: config_field_bool(config, "bootstrap"),
        snapshot_interval: config_field_u64(config, "snapshot_interval"),
        batch_max_ops: config_field_usize(config, "batch_max_ops"),
        batch_max_ms: config_field_u64(config, "batch_max_ms"),
        queue_cap: config_field_usize(config, "queue_cap"),
        blob_threshold_bytes: config_field_usize(config, "blob_threshold_bytes"),
        blob_path: config_field_string(config, "blob_path"),
        backup_enabled: config_field_bool(config, "backup_enabled"),
        backup_max_age_secs: config_field_u64(config, "backup_max_age_secs"),
        backup_max_logs: config_field_usize(config, "backup_max_logs"),
        backup_retention_days: config_field_u64(config, "backup_retention_days"),
        backup_max_keep: config_field_usize(config, "backup_max_keep"),
        backup_prefix: config_field_string(config, "backup_prefix"),
        backup_only_leader: config_field_bool(config, "backup_only_leader"),
        backup_restore_mode: config_field_string(config, "backup_restore_mode"),
        backup_restore_id: config_field_string(config, "backup_restore_id"),
        s3_bucket: config_field_string(config, "s3_bucket"),
        s3_region: config_field_string(config, "s3_region"),
        s3_access_key: config_field_string(config, "s3_access_key"),
        s3_secret_key: config_field_string(config, "s3_secret_key"),
        s3_endpoint: config_field_string(config, "s3_endpoint"),
        s3_prefix: config_field_string(config, "s3_prefix"),
    };
    config::set_storage_user_config(user);
    crate::schedule::ensure_scheduler_started();
    ok_value(Value::nil())
}

fn config_field_bool(config: Value, field: &str) -> Option<bool> {
    let val = class::class_get(config, field.as_ptr(), field.len());
    if val.is_nil() {
        unsafe { wr_rc_dec(val) };
        return None;
    }
    let out = if val.is_bool() { Some(val.as_bool()) } else { None };
    unsafe { wr_rc_dec(val) };
    out
}

fn config_field_u64(config: Value, field: &str) -> Option<u64> {
    let val = class::class_get(config, field.as_ptr(), field.len());
    if val.is_nil() {
        unsafe { wr_rc_dec(val) };
        return None;
    }
    let out = crate::value::int_value(val).and_then(|num| if num >= 0 { Some(num as u64) } else { None });
    unsafe { wr_rc_dec(val) };
    out
}

fn config_field_usize(config: Value, field: &str) -> Option<usize> {
    config_field_u64(config, field).and_then(|val| if val >= 1 { Some(val as usize) } else { None })
}

fn config_field_string(config: Value, field: &str) -> Option<String> {
    let val = class::class_get(config, field.as_ptr(), field.len());
    if val.is_nil() {
        unsafe { wr_rc_dec(val) };
        return None;
    }
    let bytes = string::with_string_bytes(val, |bytes| bytes.to_vec());
    unsafe { wr_rc_dec(val) };
    bytes.and_then(|bytes| {
        let value = String::from_utf8_lossy(&bytes).to_string();
        if value.trim().is_empty() {
            None
        } else {
            Some(value)
        }
    })
}
