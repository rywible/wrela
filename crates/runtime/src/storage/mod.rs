pub mod config;
pub mod service;
pub mod store;
mod transport;

#[cfg(test)]
mod tests;

use crate::actor::{pending_new, resolve_pending, runtime_spawn};
use crate::result;
use crate::string;
use crate::value::Value;
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
