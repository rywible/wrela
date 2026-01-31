use crate::actor::{
    actor_class_id, actor_send, pending_await_async, pending_new, resolve_pending, runtime_spawn,
};
use crate::http::method_id_for;
use crate::list;
use crate::map;
use crate::result;
use crate::storage_helpers::{storage_get_json_vec, storage_set_json, value_to_string};
use crate::string;
use crate::value::{TypeId, Value, int_value, type_id_raw};
use crate::{wr_rc_dec, wr_rc_inc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use uuid::Uuid;

#[derive(Clone)]
struct JobItem {
    id: String,
    payload: Value,
    attempts: u32,
    max_retries: u32,
    backoff_secs: u64,
    enqueued_at: u64,
}

#[derive(Clone, Serialize, Deserialize)]
struct StoredJob {
    id: String,
    payload: JsonValue,
    attempts: u32,
    max_retries: u32,
    backoff_secs: u64,
    enqueued_at: u64,
}

struct QueueState {
    sender: Option<mpsc::Sender<JobItem>>,
    backlog: VecDeque<JobItem>,
    handler: Option<Value>,
    method_id: Option<u32>,
}

struct JobsState {
    queues: HashMap<String, QueueState>,
}

static STATE: OnceLock<Mutex<JobsState>> = OnceLock::new();

fn jobs_state() -> &'static Mutex<JobsState> {
    STATE.get_or_init(|| {
        Mutex::new(JobsState {
            queues: HashMap::new(),
        })
    })
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

fn json_from_map(val: Value) -> JsonValue {
    let Some(map_ptr) = map::as_map_ref(val) else {
        return JsonValue::Null;
    };
    let mut out = serde_json::Map::new();
    unsafe {
        for (key, value) in (&(*map_ptr).entries).iter() {
            let Some(key_str) = value_to_string(key.0) else {
                continue;
            };
            let json_val = if value.is_nil() {
                JsonValue::Null
            } else if value.is_bool() {
                JsonValue::Bool(value.as_bool())
            } else if let Some(i) = int_value(*value) {
                JsonValue::Number(i.into())
            } else if value.is_float() {
                serde_json::Number::from_f64(value.as_float())
                    .map(JsonValue::Number)
                    .unwrap_or(JsonValue::Null)
            } else if let Some(s) = value_to_string(*value) {
                JsonValue::String(s)
            } else {
                JsonValue::Null
            };
            out.insert(key_str, json_val);
        }
    }
    JsonValue::Object(out)
}

fn map_from_json(value: &JsonValue) -> Value {
    let map_val = map::map_new();
    let JsonValue::Object(obj) = value else {
        return map_val;
    };
    for (key, val) in obj {
        let key_val = string::str_from_bytes(key.as_bytes());
        let out_val = match val {
            JsonValue::Null => Value::nil(),
            JsonValue::Bool(b) => Value::from_bool(*b),
            JsonValue::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Value::from_int(i)
                } else if let Some(f) = n.as_f64() {
                    Value::from_float(f)
                } else {
                    Value::nil()
                }
            }
            JsonValue::String(s) => string::str_from_bytes(s.as_bytes()),
            JsonValue::Array(_) | JsonValue::Object(_) => Value::nil(),
        };
        map::map_set(map_val, key_val, out_val);
        unsafe {
            wr_rc_dec(key_val);
            if out_val.is_ptr() {
                wr_rc_dec(out_val);
            }
        }
    }
    map_val
}

async fn load_queue(queue: &str) -> Vec<StoredJob> {
    let key = format!("jobs:queue:{queue}");
    storage_get_json_vec(&key).await
}

async fn save_queue(queue: &str, jobs: &[StoredJob]) -> bool {
    let key = format!("jobs:queue:{queue}");
    storage_set_json(&key, jobs).await
}

async fn load_dlq(queue: &str) -> Vec<StoredJob> {
    let key = format!("jobs:dlq:{queue}");
    storage_get_json_vec(&key).await
}

async fn save_dlq(queue: &str, jobs: &[StoredJob]) -> bool {
    let key = format!("jobs:dlq:{queue}");
    storage_set_json(&key, jobs).await
}

fn queue_state_mut<'a>(guard: &'a mut JobsState, name: &str) -> &'a mut QueueState {
    guard
        .queues
        .entry(name.to_string())
        .or_insert_with(|| QueueState {
            sender: None,
            backlog: VecDeque::new(),
            handler: None,
            method_id: None,
        })
}

pub fn jobs_enqueue(storage: Value, queue: Value, payload: Value, opts: Value) -> Value {
    let (pending, state) = pending_new();
    if storage.is_nil() {
        resolve_pending(state, Value::nil());
        return pending;
    }
    let queue = match value_to_string(queue) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::nil());
            return pending;
        }
    };
    let delay_secs = map_get_int(opts, "delay_secs").unwrap_or(0).max(0) as u64;
    let max_retries = map_get_int(opts, "max_retries").unwrap_or(3).max(0) as u32;
    let backoff_secs = map_get_int(opts, "backoff").unwrap_or(1).max(0) as u64;
    unsafe { wr_rc_inc(payload) };
    runtime_spawn(async move {
        let job_id = Uuid::new_v4().to_string();
        let job = JobItem {
            id: job_id.clone(),
            payload,
            attempts: 0,
            max_retries,
            backoff_secs,
            enqueued_at: now_secs(),
        };
        let stored = StoredJob {
            id: job_id.clone(),
            payload: json_from_map(job.payload),
            attempts: 0,
            max_retries,
            backoff_secs,
            enqueued_at: job.enqueued_at,
        };
        let mut stored_queue = load_queue(&queue).await;
        stored_queue.push(stored.clone());
        let _ = save_queue(&queue, &stored_queue).await;
        let mut guard = jobs_state().lock().expect("jobs state lock");
        let queue_state = queue_state_mut(&mut guard, &queue);
        if let Some(sender) = queue_state.sender.clone() {
            let job_clone = job.clone();
            drop(guard);
            if delay_secs > 0 {
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(delay_secs)).await;
                    let _ = sender.send(job_clone).await;
                });
            } else {
                let _ = sender.try_send(job);
            }
        } else {
            queue_state.backlog.push_back(job);
        }
        resolve_pending(state, string::str_from_bytes(job_id.as_bytes()));
    });
    pending
}

pub fn jobs_process(storage: Value, queue: Value, handler: Value) -> Value {
    let (pending, state) = pending_new();
    if storage.is_nil() {
        resolve_pending(state, Value::from_bool(false));
        return pending;
    }
    let queue_name = match value_to_string(queue) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::from_bool(false));
            return pending;
        }
    };
    let Some(class_id) = actor_class_id(handler) else {
        resolve_pending(state, Value::from_bool(false));
        return pending;
    };
    let Some(method_id) = method_id_for(class_id, "handle") else {
        resolve_pending(state, Value::from_bool(false));
        return pending;
    };
    unsafe { wr_rc_inc(handler) };
    runtime_spawn(async move {
        let (sender, mut rx) = mpsc::channel::<JobItem>(1024);
        {
            let mut guard = jobs_state().lock().expect("jobs state lock");
            let queue_state = queue_state_mut(&mut guard, &queue_name);
            queue_state.sender = Some(sender.clone());
            if let Some(old) = queue_state.handler.take() {
                unsafe { wr_rc_dec(old) };
            }
            queue_state.handler = Some(handler);
            queue_state.method_id = Some(method_id);
            let backlog = std::mem::take(&mut queue_state.backlog);
            for job in backlog {
                let _ = sender.try_send(job);
            }
        }

        let stored_backlog = load_queue(&queue_name).await;
        for stored in stored_backlog {
            let payload_val = map_from_json(&stored.payload);
            let job = JobItem {
                id: stored.id,
                payload: payload_val,
                attempts: stored.attempts,
                max_retries: stored.max_retries,
                backoff_secs: stored.backoff_secs,
                enqueued_at: stored.enqueued_at,
            };
            let _ = sender.try_send(job);
        }

        while let Some(mut job) = rx.recv().await {
            let args = [job.payload];
            let pending_val = actor_send(handler, method_id, 1, args.as_ptr());
            let result = pending_await_async(pending_val).await;
            let success = if type_id_raw(result) == TypeId::Result as u32 {
                let inner = result::result_unwrap(result);
                let success = if type_id_raw(inner) == TypeId::Result as u32 {
                    let ok_val = result::result_is_ok(inner);
                    ok_val.is_bool() && ok_val.as_bool()
                } else {
                    !inner.is_nil()
                };
                if inner.is_ptr() {
                    unsafe { wr_rc_dec(inner) };
                }
                success
            } else {
                !result.is_nil()
            };
            unsafe {
                wr_rc_dec(pending_val);
                if result.is_ptr() {
                    wr_rc_dec(result);
                }
            }
            if success {
                let mut stored_queue = load_queue(&queue_name).await;
                stored_queue.retain(|entry| entry.id != job.id);
                let _ = save_queue(&queue_name, &stored_queue).await;
                unsafe { wr_rc_dec(job.payload) };
                continue;
            }
            job.attempts += 1;
            if job.attempts > job.max_retries {
                let mut stored_queue = load_queue(&queue_name).await;
                stored_queue.retain(|entry| entry.id != job.id);
                let _ = save_queue(&queue_name, &stored_queue).await;
                let mut dlq = load_dlq(&queue_name).await;
                dlq.push(StoredJob {
                    id: job.id.clone(),
                    payload: json_from_map(job.payload),
                    attempts: job.attempts,
                    max_retries: job.max_retries,
                    backoff_secs: job.backoff_secs,
                    enqueued_at: job.enqueued_at,
                });
                let _ = save_dlq(&queue_name, &dlq).await;
                continue;
            }
            let mut stored_queue = load_queue(&queue_name).await;
            if let Some(entry) = stored_queue.iter_mut().find(|entry| entry.id == job.id) {
                entry.attempts = job.attempts;
            }
            let _ = save_queue(&queue_name, &stored_queue).await;
            let retry_sender = sender.clone();
            let delay = job.backoff_secs * (job.attempts as u64);
            tokio::spawn(async move {
                if delay > 0 {
                    tokio::time::sleep(Duration::from_secs(delay)).await;
                }
                let _ = retry_sender.send(job).await;
            });
        }
    });
    resolve_pending(state, Value::from_bool(true));
    pending
}

pub fn jobs_dead_letter(storage: Value, queue: Value) -> Value {
    let (pending, state) = pending_new();
    if storage.is_nil() {
        resolve_pending(state, Value::nil());
        return pending;
    }
    let queue = match value_to_string(queue) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::nil());
            return pending;
        }
    };
    runtime_spawn(async move {
        let list_val = list::list_new(0);
        let dlq = load_dlq(&queue).await;
        for job in dlq {
            let map_val = map::map_new();
            map_set_string(map_val, "id", &job.id);
            map_set_int(map_val, "attempts", job.attempts as i64);
            map_set_int(map_val, "max_retries", job.max_retries as i64);
            map_set_int(map_val, "enqueued_at", job.enqueued_at as i64);
            let key_payload = string::str_from_bytes(b"payload");
            let payload_val = map_from_json(&job.payload);
            map::map_set(map_val, key_payload, payload_val);
            unsafe {
                wr_rc_dec(key_payload);
                wr_rc_dec(payload_val);
            };
            list::list_push(list_val, map_val);
            unsafe { wr_rc_dec(map_val) };
        }
        resolve_pending(state, list_val);
    });
    pending
}
