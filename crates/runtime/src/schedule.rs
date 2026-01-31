use crate::actor::{actor_class_id, actor_send, pending_await_async, pending_new, resolve_pending, runtime_spawn};
use crate::http::method_id_for;
use crate::storage_helpers::{storage_get_json_vec, storage_set_json, value_to_string};
use crate::value::Value;
use crate::wr_rc_inc;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Clone, Serialize, Deserialize)]
struct ScheduleEntry {
    kind: String,
    expr: Option<String>,
    seconds: Option<i64>,
    timestamp: Option<i64>,
}

fn schedule_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

async fn append_schedule(entry: ScheduleEntry) {
    let _guard = schedule_lock().lock().await;
    let mut entries = storage_get_json_vec::<ScheduleEntry>("schedule:entries").await;
    entries.push(entry);
    let _ = storage_set_json("schedule:entries", &entries).await;
}

fn spawn_job(handler: Value, method_id: u32) {
    runtime_spawn(async move {
        let args = [];
        let pending = actor_send(handler, method_id, 0, args.as_ptr());
        let result = pending_await_async(pending).await;
        if pending.is_ptr() {
            unsafe { crate::wr_rc_dec(pending) };
        }
        if result.is_ptr() {
            unsafe { crate::wr_rc_dec(result) };
        }
    });
}

pub fn schedule_every(storage: Value, seconds: Value, job: Value) -> Value {
    let (pending, state) = pending_new();
    if storage.is_nil() {
        resolve_pending(state, Value::from_bool(false));
        return pending;
    }
    let Some(secs) = crate::value::int_value(seconds) else {
        resolve_pending(state, Value::from_bool(false));
        return pending;
    };
    let secs = secs.max(1) as u64;
    let Some(class_id) = actor_class_id(job) else {
        resolve_pending(state, Value::from_bool(false));
        return pending;
    };
    let Some(method_id) = method_id_for(class_id, "handle") else {
        resolve_pending(state, Value::from_bool(false));
        return pending;
    };
    unsafe { wr_rc_inc(job) };
    runtime_spawn(async move {
        append_schedule(ScheduleEntry {
            kind: "every".to_string(),
            expr: None,
            seconds: Some(secs as i64),
            timestamp: None,
        })
        .await;
        let mut interval = tokio::time::interval(Duration::from_secs(secs));
        loop {
            interval.tick().await;
            spawn_job(job, method_id);
        }
    });
    resolve_pending(state, Value::from_bool(true));
    pending
}

pub fn schedule_at(storage: Value, timestamp: Value, job: Value) -> Value {
    let (pending, state) = pending_new();
    if storage.is_nil() {
        resolve_pending(state, Value::from_bool(false));
        return pending;
    }
    let Some(ts) = crate::value::int_value(timestamp) else {
        resolve_pending(state, Value::from_bool(false));
        return pending;
    };
    let Some(class_id) = actor_class_id(job) else {
        resolve_pending(state, Value::from_bool(false));
        return pending;
    };
    let Some(method_id) = method_id_for(class_id, "handle") else {
        resolve_pending(state, Value::from_bool(false));
        return pending;
    };
    unsafe { wr_rc_inc(job) };
    let ts = ts.max(0) as u64;
    runtime_spawn(async move {
        append_schedule(ScheduleEntry {
            kind: "at".to_string(),
            expr: None,
            seconds: None,
            timestamp: Some(ts as i64),
        })
        .await;
        let now = now_secs();
        if ts > now {
            tokio::time::sleep(Duration::from_secs(ts - now)).await;
        }
        spawn_job(job, method_id);
    });
    resolve_pending(state, Value::from_bool(true));
    pending
}

pub fn schedule_cron(storage: Value, expr: Value, job: Value) -> Value {
    let (pending, state) = pending_new();
    if storage.is_nil() {
        resolve_pending(state, Value::from_bool(false));
        return pending;
    }
    let Some(expr) = value_to_string(expr) else {
        resolve_pending(state, Value::from_bool(false));
        return pending;
    };
    let Some(class_id) = actor_class_id(job) else {
        resolve_pending(state, Value::from_bool(false));
        return pending;
    };
    let Some(method_id) = method_id_for(class_id, "handle") else {
        resolve_pending(state, Value::from_bool(false));
        return pending;
    };
    let interval_secs = if expr.trim() == "* * * * *" {
        60
    } else if let Some(stripped) = expr.trim().strip_prefix("*/") {
        stripped
            .split_whitespace()
            .next()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(1)
            .max(1)
            * 60
    } else {
        60
    };
    unsafe { wr_rc_inc(job) };
    runtime_spawn(async move {
        append_schedule(ScheduleEntry {
            kind: "cron".to_string(),
            expr: Some(expr.clone()),
            seconds: None,
            timestamp: None,
        })
        .await;
        let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
        loop {
            interval.tick().await;
            spawn_job(job, method_id);
        }
    });
    resolve_pending(state, Value::from_bool(true));
    pending
}
