pub mod config;
pub mod blob;
mod backup;
pub mod service;
pub mod store;
pub mod value;
mod transport;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod backup_tests;

use crate::actor::{pending_new, resolve_pending, runtime_spawn};
use crate::class;
use crate::result;
use crate::string;
use crate::value::Value;
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

fn resolve_response(resp: StorageResponse) -> Value {
    match resp {
        StorageResponse::Ok(value) => ok_value(value),
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

pub fn storage_configure(config: Value) -> Value {
    let user = StorageUserConfig {
        file_path: config_field_string(config, "file_path"),
        s3_bucket: config_field_string(config, "s3_bucket"),
        s3_region: config_field_string(config, "s3_region"),
        s3_access_key: config_field_string(config, "s3_access_key"),
        s3_secret_key: config_field_string(config, "s3_secret_key"),
        s3_endpoint: config_field_string(config, "s3_endpoint"),
        s3_prefix: config_field_string(config, "s3_prefix"),
    };
    config::set_storage_user_config(user);
    ok_value(Value::nil())
}

fn config_field_string(config: Value, field: &str) -> Option<String> {
    let val = class::class_get(config, field.as_ptr(), field.len());
    if val.is_nil() {
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
