use crate::actor::{pending_new, resolve_pending, runtime_spawn};
use crate::bytes;
use crate::map;
use crate::storage_helpers::{
    storage_delete, storage_get_json, storage_set_json, storage_set_string, value_to_string,
};
use crate::string;
use crate::value::{Value, int_value};
use crate::wr_rc_dec;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
struct FileMeta {
    id: String,
    owner_id: Option<String>,
    acl: String,
    size: u64,
    content_type: Option<String>,
    created_at: u64,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn map_get_string(map_val: Value, key: &str) -> Option<String> {
    let key_val = string::str_from_bytes(key.as_bytes());
    let got = map::map_get(map_val, key_val);
    unsafe { wr_rc_dec(key_val) };
    if got.is_nil() {
        return None;
    }
    let out = value_to_string(got);
    unsafe { wr_rc_dec(got) };
    out
}

fn map_get_int(map_val: Value, key: &str) -> Option<i64> {
    let key_val = string::str_from_bytes(key.as_bytes());
    let got = map::map_get(map_val, key_val);
    unsafe { wr_rc_dec(key_val) };
    let out = int_value(got);
    unsafe { wr_rc_dec(got) };
    out
}

fn map_set_string(map_val: Value, key: &str, value: &str) {
    let key_val = string::str_from_bytes(key.as_bytes());
    let val = string::str_from_bytes(value.as_bytes());
    map::map_set(map_val, key_val, val);
    unsafe {
        wr_rc_dec(key_val);
        wr_rc_dec(val);
    }
}

fn map_set_int(map_val: Value, key: &str, value: i64) {
    let key_val = string::str_from_bytes(key.as_bytes());
    map::map_set(map_val, key_val, Value::from_int(value));
    unsafe { wr_rc_dec(key_val) };
}

pub fn files_upload_stream(storage: Value, stream: Value, opts: Value) -> Value {
    let (pending, state) = pending_new();
    if storage.is_nil() {
        resolve_pending(state, Value::nil());
        return pending;
    }
    let bytes = bytes::with_bytes(stream, |b| b.to_vec()).unwrap_or_default();
    let acl = map_get_string(opts, "acl").unwrap_or_else(|| "private".to_string());
    let owner_id = map_get_string(opts, "owner_id");
    let content_type = map_get_string(opts, "content_type");
    runtime_spawn(async move {
        let id = Uuid::new_v4().to_string();
        let meta = FileMeta {
            id: id.clone(),
            owner_id,
            acl,
            size: bytes.len() as u64,
            content_type,
            created_at: now_secs(),
        };
        let meta_key = format!("files:{id}");
        let blob_key = format!("files:blob:{id}");
        let encoded = STANDARD.encode(bytes);
        let stored = storage_set_json(&meta_key, &meta).await
            && storage_set_string(&blob_key, &encoded).await;
        if !stored {
            resolve_pending(state, Value::nil());
            return;
        }
        resolve_pending(state, string::str_from_bytes(id.as_bytes()));
    });
    pending
}

pub fn files_signed_url(storage: Value, file_id: Value, opts: Value) -> Value {
    let (pending, state) = pending_new();
    if storage.is_nil() {
        resolve_pending(state, Value::nil());
        return pending;
    }
    let file_id = match value_to_string(file_id) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::nil());
            return pending;
        }
    };
    let ttl = map_get_int(opts, "ttl").unwrap_or(3600).max(1) as u64;
    let method = map_get_string(opts, "method").unwrap_or_else(|| "GET".to_string());
    runtime_spawn(async move {
        let meta_key = format!("files:{file_id}");
        if storage_get_json::<FileMeta>(&meta_key).await.is_none() {
            resolve_pending(state, Value::nil());
            return;
        }
        let exp = now_secs() + ttl;
        let token = Uuid::new_v4().to_string();
        let url = format!("wrela://files/{file_id}?token={token}&exp={exp}&method={method}");
        resolve_pending(state, string::str_from_bytes(url.as_bytes()));
    });
    pending
}

pub fn files_metadata(storage: Value, file_id: Value) -> Value {
    let (pending, state) = pending_new();
    if storage.is_nil() {
        resolve_pending(state, Value::nil());
        return pending;
    }
    let file_id = match value_to_string(file_id) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::nil());
            return pending;
        }
    };
    runtime_spawn(async move {
        let meta_key = format!("files:{file_id}");
        let Some(meta) = storage_get_json::<FileMeta>(&meta_key).await else {
            resolve_pending(state, Value::nil());
            return;
        };
        let map_val = map::map_new();
        map_set_string(map_val, "id", &meta.id);
        map_set_string(map_val, "acl", &meta.acl);
        if let Some(owner) = &meta.owner_id {
            map_set_string(map_val, "owner_id", owner);
        }
        if let Some(content_type) = &meta.content_type {
            map_set_string(map_val, "content_type", content_type);
        }
        map_set_int(map_val, "size", meta.size as i64);
        map_set_int(map_val, "created_at", meta.created_at as i64);
        resolve_pending(state, map_val);
    });
    pending
}

pub fn files_delete(storage: Value, file_id: Value) -> Value {
    let (pending, state) = pending_new();
    if storage.is_nil() {
        resolve_pending(state, Value::from_bool(false));
        return pending;
    }
    let file_id = match value_to_string(file_id) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::from_bool(false));
            return pending;
        }
    };
    runtime_spawn(async move {
        let meta_key = format!("files:{file_id}");
        let blob_key = format!("files:blob:{file_id}");
        let existed = storage_get_json::<FileMeta>(&meta_key).await.is_some();
        let _ = storage_delete(&meta_key).await;
        let _ = storage_delete(&blob_key).await;
        resolve_pending(state, Value::from_bool(existed));
    });
    pending
}

pub fn files_set_acl(storage: Value, file_id: Value, acl: Value) -> Value {
    let (pending, state) = pending_new();
    if storage.is_nil() {
        resolve_pending(state, Value::from_bool(false));
        return pending;
    }
    let file_id = match value_to_string(file_id) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::from_bool(false));
            return pending;
        }
    };
    let acl = match value_to_string(acl) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::from_bool(false));
            return pending;
        }
    };
    runtime_spawn(async move {
        let meta_key = format!("files:{file_id}");
        let mut meta = match storage_get_json::<FileMeta>(&meta_key).await {
            Some(meta) => meta,
            None => {
                resolve_pending(state, Value::from_bool(false));
                return;
            }
        };
        meta.acl = acl;
        let ok = storage_set_json(&meta_key, &meta).await;
        resolve_pending(state, Value::from_bool(ok));
    });
    pending
}
