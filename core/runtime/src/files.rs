use crate::actor::{pending_new, resolve_pending, runtime_spawn};
use crate::bytes;
use crate::map;
use crate::storage::config::{storage_config, S3Config};
use crate::storage_helpers::{
    storage_delete, storage_get_json, storage_set_bytes, storage_set_json, value_to_string,
};
use crate::string;
use crate::value::{int_value, Value};
use crate::wr_rc_dec;
use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::presigning::PresigningConfig;
use aws_config::meta::region::RegionProviderChain;
use aws_types::region::Region;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::OnceCell;
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
struct FileMeta {
    id: String,
    owner_id: Option<String>,
    acl: String,
    size: u64,
    content_type: Option<String>,
    created_at: u64,
    backend: Option<String>,
    blob_key: Option<String>,
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

fn s3_client_cell() -> &'static OnceCell<aws_sdk_s3::Client> {
    static CELL: OnceLock<OnceCell<aws_sdk_s3::Client>> = OnceLock::new();
    CELL.get_or_init(OnceCell::new)
}

async fn s3_client(cfg: &S3Config) -> aws_sdk_s3::Client {
    s3_client_cell()
        .get_or_init(|| async move {
            let creds = Credentials::new(
                cfg.access_key.clone(),
                cfg.secret_key.clone(),
                None,
                None,
                "wrela-files",
            );
            let region = Region::new(cfg.region.clone());
            let region_provider = RegionProviderChain::first_try(region).or_default_provider();
            let mut loader = aws_config::defaults(BehaviorVersion::latest())
                .region(region_provider)
                .credentials_provider(creds);
            if let Some(endpoint) = cfg.endpoint.as_ref() {
                loader = loader.endpoint_url(endpoint);
            }
            let shared = loader.load().await;
            aws_sdk_s3::Client::new(&shared)
        })
        .await
        .clone()
}

fn s3_object_key(cfg: &S3Config, key: &str) -> String {
    match cfg.prefix.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        Some(prefix) => format!("{prefix}/{key}"),
        None => key.to_string(),
    }
}

fn s3_method(method: &str) -> Option<&str> {
    match method.to_ascii_uppercase().as_str() {
        "GET" => Some("GET"),
        "PUT" => Some("PUT"),
        _ => None,
    }
}

fn parse_file_id_with_requester(file_id: Value) -> Option<(String, Option<String>)> {
    if map::as_map_ref(file_id).is_some() {
        let id = map_get_string(file_id, "id")?;
        let requester =
            map_get_string(file_id, "requester_id").or_else(|| map_get_string(file_id, "owner_id"));
        return Some((id, requester));
    }
    value_to_string(file_id).map(|id| (id, None))
}

fn parse_acl_with_requester(acl_val: Value) -> Option<(String, Option<String>)> {
    if let Some(acl) = value_to_string(acl_val) {
        return Some((acl, None));
    }
    if map::as_map_ref(acl_val).is_some() {
        let acl = map_get_string(acl_val, "acl")?;
        let requester =
            map_get_string(acl_val, "requester_id").or_else(|| map_get_string(acl_val, "owner_id"));
        return Some((acl, requester));
    }
    None
}

fn requester_allowed(meta: &FileMeta, requester: Option<&str>) -> bool {
    if meta.acl == "public" {
        return true;
    }
    let Some(owner) = meta.owner_id.as_deref() else {
        return false;
    };
    let Some(requester) = requester else {
        return false;
    };
    requester == owner
}

fn requester_is_owner(meta: &FileMeta, requester: Option<&str>) -> bool {
    let Some(owner) = meta.owner_id.as_deref() else {
        return false;
    };
    let Some(requester) = requester else {
        return false;
    };
    requester == owner
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
        let s3 = storage_config().blob.s3.clone();
        let mut meta = FileMeta {
            id: id.clone(),
            owner_id,
            acl,
            size: bytes.len() as u64,
            content_type,
            created_at: now_secs(),
            backend: None,
            blob_key: None,
        };
        let meta_key = format!("files:{id}");
        if let Some(cfg) = s3 {
            let client = s3_client(&cfg).await;
            let object_key = s3_object_key(&cfg, &format!("files/{id}"));
            let mut request = client
                .put_object()
                .bucket(&cfg.bucket)
                .key(&object_key)
                .body(ByteStream::from(bytes));
            if let Some(content_type) = meta.content_type.as_ref() {
                request = request.content_type(content_type);
            }
            if request.send().await.is_err() {
                resolve_pending(state, Value::nil());
                return;
            }
            meta.backend = Some("s3".to_string());
            meta.blob_key = Some(object_key);
            if !storage_set_json(&meta_key, &meta).await {
                resolve_pending(state, Value::nil());
                return;
            }
        } else {
            let blob_key = format!("files:blob:{id}");
            let stored =
                storage_set_json(&meta_key, &meta).await && storage_set_bytes(&blob_key, &bytes).await;
            if !stored {
                resolve_pending(state, Value::nil());
                return;
            }
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
    let (file_id, requester_id) = match parse_file_id_with_requester(file_id) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::nil());
            return pending;
        }
    };
    let ttl = map_get_int(opts, "ttl").unwrap_or(3600).max(1) as u64;
    let method = map_get_string(opts, "method").unwrap_or_else(|| "GET".to_string());
    let requester_id = map_get_string(opts, "requester_id")
        .or_else(|| map_get_string(opts, "owner_id"))
        .or(requester_id);
    runtime_spawn(async move {
        let meta_key = format!("files:{file_id}");
        let Some(meta) = storage_get_json::<FileMeta>(&meta_key).await else {
            resolve_pending(state, Value::nil());
            return;
        };
        if !requester_allowed(&meta, requester_id.as_deref()) {
            resolve_pending(state, Value::nil());
            return;
        }
        if meta.backend.as_deref() == Some("s3") {
            let Some(blob_key) = meta.blob_key.as_ref() else {
                resolve_pending(state, Value::nil());
                return;
            };
            let Some(cfg) = storage_config().blob.s3.clone() else {
                resolve_pending(state, Value::nil());
                return;
            };
            let Some(method) = s3_method(&method) else {
                resolve_pending(state, Value::nil());
                return;
            };
            let client = s3_client(&cfg).await;
            let presign = PresigningConfig::expires_in(Duration::from_secs(ttl)).ok();
            let Some(presign) = presign else {
                resolve_pending(state, Value::nil());
                return;
            };
            let url = if method == "PUT" {
                client
                    .put_object()
                    .bucket(&cfg.bucket)
                    .key(blob_key)
                    .presigned(presign)
                    .await
                    .ok()
                    .map(|req| req.uri().to_string())
            } else {
                client
                    .get_object()
                    .bucket(&cfg.bucket)
                    .key(blob_key)
                    .presigned(presign)
                    .await
                    .ok()
                    .map(|req| req.uri().to_string())
            };
            if let Some(url) = url {
                resolve_pending(state, string::str_from_bytes(url.as_bytes()));
                return;
            }
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
    let (file_id, requester) = match parse_file_id_with_requester(file_id) {
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
        if !requester_allowed(&meta, requester.as_deref()) {
            resolve_pending(state, Value::nil());
            return;
        }
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
    let (file_id, requester) = match parse_file_id_with_requester(file_id) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::from_bool(false));
            return pending;
        }
    };
    runtime_spawn(async move {
        let meta_key = format!("files:{file_id}");
        let mut existed = false;
        if let Some(meta) = storage_get_json::<FileMeta>(&meta_key).await {
            if !requester_is_owner(&meta, requester.as_deref()) {
                resolve_pending(state, Value::from_bool(false));
                return;
            }
            existed = true;
            if meta.backend.as_deref() == Some("s3") {
                if let (Some(cfg), Some(key)) = (storage_config().blob.s3.clone(), meta.blob_key) {
                    let client = s3_client(&cfg).await;
                    let _ = client.delete_object().bucket(cfg.bucket).key(key).send().await;
                }
            } else {
                let blob_key = format!("files:blob:{file_id}");
                let _ = storage_delete(&blob_key).await;
            }
        }
        let _ = storage_delete(&meta_key).await;
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
    let (file_id, requester_from_id) = match parse_file_id_with_requester(file_id) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::from_bool(false));
            return pending;
        }
    };
    let (acl, requester_from_acl) = match parse_acl_with_requester(acl) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::from_bool(false));
            return pending;
        }
    };
    let requester = requester_from_acl.or(requester_from_id);
    runtime_spawn(async move {
        let meta_key = format!("files:{file_id}");
        let mut meta = match storage_get_json::<FileMeta>(&meta_key).await {
            Some(meta) => meta,
            None => {
                resolve_pending(state, Value::from_bool(false));
                return;
            }
        };
        if !requester_is_owner(&meta, requester.as_deref()) {
            resolve_pending(state, Value::from_bool(false));
            return;
        }
        meta.acl = acl;
        let ok = storage_set_json(&meta_key, &meta).await;
        resolve_pending(state, Value::from_bool(ok));
    });
    pending
}
