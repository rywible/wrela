use crate::actor::{pending_new, resolve_pending, runtime_spawn};
use crate::map;
use crate::storage_helpers::{storage_get_json, storage_set_json, value_to_string};
use crate::string;
use crate::value::{int_value, Value};
use crate::wr_rc_dec;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Serialize, Deserialize)]
struct Bucket {
    tokens: f64,
    last: u64,
    burst: f64,
    per_secs: f64,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn map_get_int(map_val: Value, key: &str) -> Option<i64> {
    let key_val = string::str_from_bytes(key.as_bytes());
    let got = map::map_get(map_val, key_val);
    unsafe { wr_rc_dec(key_val) };
    let out = int_value(got);
    unsafe { wr_rc_dec(got) };
    out
}

pub fn rate_check(storage: Value, key: Value, opts: Value) -> Value {
    let (pending, state) = pending_new();
    if storage.is_nil() {
        resolve_pending(state, Value::from_bool(false));
        return pending;
    }
    let key = match value_to_string(key) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::from_bool(false));
            return pending;
        }
    };
    let burst = map_get_int(opts, "burst").unwrap_or(10).max(1) as f64;
    let per_secs = map_get_int(opts, "per_secs").unwrap_or(60).max(1) as f64;
    runtime_spawn(async move {
        let now = now_secs();
        let bucket_key = format!("rate:{key}");
        let mut bucket = storage_get_json::<Bucket>(&bucket_key).await.unwrap_or(Bucket {
            tokens: burst,
            last: now,
            burst,
            per_secs,
        });
        let elapsed = (now.saturating_sub(bucket.last)) as f64;
        let rate = bucket.burst / bucket.per_secs;
        bucket.tokens = (bucket.tokens + elapsed * rate).min(bucket.burst);
        bucket.last = now;
        let ok = if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        };
        let _ = storage_set_json(&bucket_key, &bucket).await;
        resolve_pending(state, Value::from_bool(ok));
    });
    pending
}

pub fn rate_ip(request: Value) -> Value {
    let headers_val = crate::class::class_get(request, b"headers".as_ptr(), 7);
    if !headers_val.is_nil() {
        let key = string::str_from_bytes(b"x-forwarded-for");
        let val = map::map_get(headers_val, key);
        unsafe { wr_rc_dec(key) };
        if let Some(ip) = value_to_string(val) {
            unsafe { wr_rc_dec(val) };
            unsafe { wr_rc_dec(headers_val) };
            return string::str_from_bytes(ip.as_bytes());
        }
        unsafe { wr_rc_dec(val) };
        let key = string::str_from_bytes(b"x-real-ip");
        let val = map::map_get(headers_val, key);
        unsafe { wr_rc_dec(key) };
        if let Some(ip) = value_to_string(val) {
            unsafe { wr_rc_dec(val) };
            unsafe { wr_rc_dec(headers_val) };
            return string::str_from_bytes(ip.as_bytes());
        }
        unsafe { wr_rc_dec(val) };
        unsafe { wr_rc_dec(headers_val) };
    }
    string::str_from_bytes(b"unknown")
}
