use crate::actor::{
    actor_class_id, actor_send, pending_await_async, pending_new, resolve_pending, runtime_spawn,
};
use crate::http::method_id_for;
use crate::lease;
use crate::list;
use crate::map;
use crate::pubsub;
use crate::result;
use crate::metrics;
use crate::storage::config::storage_config;
use crate::storage_helpers::{
    storage_delete, storage_get_string, storage_list_prefix_keys, storage_set_json,
    storage_set_string_if_version, value_to_string,
};
use crate::string;
use crate::value::{TypeId, Value, int_value, type_id_raw};
use crate::{wr_rc_dec, wr_rc_inc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet, VecDeque};
#[cfg(any(test, feature = "test-utils"))]
use std::future::Future;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use uuid::Uuid;

#[derive(Clone)]
struct JobsConfig {
    lease_ttl_secs: u64,
}

impl Default for JobsConfig {
    fn default() -> Self {
        Self { lease_ttl_secs: 60 }
    }
}

static JOBS_CONFIG: OnceLock<Mutex<JobsConfig>> = OnceLock::new();

fn jobs_config() -> JobsConfig {
    JOBS_CONFIG
        .get_or_init(|| Mutex::new(JobsConfig::default()))
        .lock()
        .expect("jobs config lock")
        .clone()
}

fn set_jobs_config(config: JobsConfig) {
    *JOBS_CONFIG
        .get_or_init(|| Mutex::new(JobsConfig::default()))
        .lock()
        .expect("jobs config lock") = config;
}

#[derive(Clone)]
struct JobItem {
    id: String,
    payload: Value,
    attempts: u32,
    max_retries: u32,
    backoff_secs: u64,
    enqueued_at: u64,
    run_at: u64,
    storage_key: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct StoredJob {
    id: String,
    payload: JsonValue,
    attempts: u32,
    max_retries: u32,
    backoff_secs: u64,
    enqueued_at: u64,
    #[serde(default)]
    run_at: u64,
}

struct QueueState {
    sender: Option<mpsc::Sender<JobItem>>,
    backlog: VecDeque<JobItem>,
    handler: Option<Value>,
    method_id: Option<u32>,
    inflight: std::collections::HashSet<String>,
}

struct JobsState {
    queues: HashMap<String, QueueState>,
    subscribed: HashSet<String>,
}

static STATE: OnceLock<Arc<Mutex<JobsState>>> = OnceLock::new();

#[cfg(any(test, feature = "test-utils"))]
tokio::task_local! {
    static JOBS_STATE_OVERRIDE: Arc<Mutex<JobsState>>;
}

fn jobs_state() -> Arc<Mutex<JobsState>> {
    #[cfg(any(test, feature = "test-utils"))]
    if let Ok(state) = JOBS_STATE_OVERRIDE.try_with(Arc::clone) {
        return state;
    }
    STATE
        .get_or_init(|| {
            Arc::new(Mutex::new(JobsState {
                queues: HashMap::new(),
                subscribed: HashSet::new(),
            }))
        })
        .clone()
}

#[cfg(any(test, feature = "test-utils"))]
#[allow(dead_code)]
pub struct JobsStateHandle(Arc<Mutex<JobsState>>);

#[cfg(any(test, feature = "test-utils"))]
#[allow(dead_code)]
pub fn new_jobs_state_for_test() -> JobsStateHandle {
    JobsStateHandle(Arc::new(Mutex::new(JobsState {
        queues: HashMap::new(),
        subscribed: HashSet::new(),
    })))
}

#[cfg(any(test, feature = "test-utils"))]
#[allow(dead_code)]
pub async fn with_jobs_state_override<F, R>(state: &JobsStateHandle, fut: F) -> R
where
    F: Future<Output = R>,
{
    JOBS_STATE_OVERRIDE.scope(state.0.clone(), fut).await
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn job_lease_ttl_secs() -> u64 {
    jobs_config().lease_ttl_secs.max(1)
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

pub fn jobs_configure(config: Value) -> Value {
    let new_config = jobs_config_from_value(config);
    set_jobs_config(new_config);
    Value::nil()
}

fn jobs_config_from_value(config: Value) -> JobsConfig {
    let mut out = JobsConfig::default();
    if let Some(val) = config_field_u64(config, "lease_ttl_secs") {
        out.lease_ttl_secs = val.max(1);
    }
    out
}

fn config_field_u64(config: Value, field: &str) -> Option<u64> {
    let val = crate::class::class_get(config, field.as_ptr(), field.len());
    if val.is_nil() {
        unsafe { wr_rc_dec(val) };
        return None;
    }
    let out = int_value(val).and_then(|num| if num >= 0 { Some(num as u64) } else { None });
    unsafe { wr_rc_dec(val) };
    out
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
    let prefix = queue_prefix(queue);
    let keys = storage_list_prefix_keys(&prefix, 1000).await;
    let mut out = Vec::new();
    for key in keys {
        let Some(raw) = storage_get_string(&key).await else {
            continue;
        };
        let Ok(mut job) = serde_json::from_str::<StoredJob>(&raw) else {
            continue;
        };
        if job.run_at == 0 {
            job.run_at = job.enqueued_at;
        }
        out.push(job);
    }
    out
}

async fn scan_due_jobs(queue: &str, sender: &mpsc::Sender<JobItem>, owner: &str) {
    let prefix = queue_prefix(queue);
    let keys = storage_list_prefix_keys(&prefix, 1000).await;
    if keys.is_empty() {
        return;
    }
    let now = now_secs();
    for key in keys {
        let Some(raw) = storage_get_string(&key).await else {
            continue;
        };
        let Ok(mut job) = serde_json::from_str::<StoredJob>(&raw) else {
            continue;
        };
        if job.run_at == 0 {
            job.run_at = job.enqueued_at;
        }
        if job.run_at > now {
            continue;
        }
        let done = storage_get_string(&done_key(queue, &job.id)).await;
        if done.is_some() {
            let _ = storage_delete(&key).await;
            continue;
        }
        let lease_key = lease_key(queue, &job.id);
        let lease_ttl = job_lease_ttl_secs();
        let acquired = lease::try_acquire_lease(&lease_key, owner, lease_ttl).await;
        if !acquired {
            continue;
        }
        let payload_val = map_from_json(&job.payload);
        let item = JobItem {
            id: job.id.clone(),
            payload: payload_val,
            attempts: job.attempts,
            max_retries: job.max_retries,
            backoff_secs: job.backoff_secs,
            enqueued_at: job.enqueued_at,
            run_at: job.run_at,
            storage_key: key.clone(),
        };
        if sender.send(item).await.is_err() {
            let _ = lease::release_lease(&lease_key, owner).await;
            unsafe {
                if payload_val.is_ptr() {
                    wr_rc_dec(payload_val);
                }
            }
        }
    }
}

async fn load_dlq(queue: &str) -> Vec<StoredJob> {
    let prefix = dlq_prefix(queue);
    let keys = storage_list_prefix_keys(&prefix, 1000).await;
    let mut out = Vec::new();
    for key in keys {
        let Some(raw) = storage_get_string(&key).await else {
            continue;
        };
        let Ok(job) = serde_json::from_str::<StoredJob>(&raw) else {
            continue;
        };
        out.push(job);
    }
    out
}

fn queue_prefix(queue: &str) -> String {
    format!("jobs:queue:{queue}:")
}

fn queue_key(queue: &str, job_id: &str) -> String {
    format!("jobs:queue:{queue}:{job_id}")
}

fn dlq_prefix(queue: &str) -> String {
    format!("jobs:dlq:{queue}:")
}

fn dlq_key(queue: &str, job_id: &str) -> String {
    format!("jobs:dlq:{queue}:{job_id}")
}

fn lease_key(queue: &str, job_id: &str) -> String {
    format!("jobs:lease:{queue}:{job_id}")
}

fn done_key(queue: &str, job_id: &str) -> String {
    format!("jobs:done:{queue}:{job_id}")
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
            inflight: std::collections::HashSet::new(),
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
        let ha_enabled = !storage_config().peers.is_empty();
        let job_id = Uuid::new_v4().to_string();
        let now = now_secs();
        let run_at = now.saturating_add(delay_secs);
        let storage_key = queue_key(&queue, &job_id);
        let job = JobItem {
            id: job_id.clone(),
            payload,
            attempts: 0,
            max_retries,
            backoff_secs,
            enqueued_at: now,
            run_at,
            storage_key: storage_key.clone(),
        };
        let stored = StoredJob {
            id: job_id.clone(),
            payload: json_from_map(job.payload),
            attempts: 0,
            max_retries,
            backoff_secs,
            enqueued_at: job.enqueued_at,
            run_at: job.run_at,
        };
        let _ = storage_set_json(&storage_key, &stored).await;
        if ha_enabled {
            let topic = format!("jobs:wakeup:{queue}");
            #[cfg(feature = "metrics")]
            metrics::inc_jobs_wakeup();
            pubsub::publish(&topic, JsonValue::Null).await;
        }
        let jobs_state_ref = jobs_state();
        let mut guard = jobs_state_ref.lock().expect("jobs state lock");
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
        let ha_enabled = !storage_config().peers.is_empty();
        let (sender, mut rx) = mpsc::channel::<JobItem>(1024);
        let owner = lease::owner_id();
        if ha_enabled {
            let scan_sender = sender.clone();
            let queue_name = queue_name.clone();
            let owner_for_scan = owner.clone();
            tokio::spawn(async move {
                loop {
                    scan_due_jobs(&queue_name, &scan_sender, &owner_for_scan).await;
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            });
        }
        let lease_ttl_default = job_lease_ttl_secs();
        {
            let jobs_state_ref = jobs_state();
            let mut guard = jobs_state_ref.lock().expect("jobs state lock");
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
        if ha_enabled {
            let should_subscribe = {
                let jobs_state_ref = jobs_state();
                let mut guard = jobs_state_ref.lock().expect("jobs state lock");
                guard.subscribed.insert(queue_name.clone())
            };
            if should_subscribe {
                let queue_name_for_pubsub = queue_name.clone();
                let topic = format!("jobs:wakeup:{queue_name_for_pubsub}");
                let scan_sender = sender.clone();
                let owner = owner.clone();
                runtime_spawn(async move {
                    pubsub::subscribe(&topic, move |_| {
                        let scan_sender = scan_sender.clone();
                        let queue_name = queue_name_for_pubsub.clone();
                        let owner = owner.clone();
                        async move {
                            scan_due_jobs(&queue_name, &scan_sender, &owner).await;
                        }
                    })
                    .await;
                });
            }
        }

        let stored_backlog = load_queue(&queue_name).await;
        for stored in stored_backlog {
            let payload_val = map_from_json(&stored.payload);
            let storage_key = queue_key(&queue_name, &stored.id);
            let job = JobItem {
                id: stored.id,
                payload: payload_val,
                attempts: stored.attempts,
                max_retries: stored.max_retries,
                backoff_secs: stored.backoff_secs,
                enqueued_at: stored.enqueued_at,
                run_at: stored.run_at,
                storage_key,
            };
            let now = now_secs();
            if job.run_at > now {
                let delay = job.run_at - now;
                let retry_sender = sender.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(delay)).await;
                    let _ = retry_sender.send(job).await;
                });
            } else {
                let _ = sender.try_send(job);
            }
        }

        while let Some(mut job) = rx.recv().await {
            let now = now_secs();
            if job.run_at > now {
                let delay = job.run_at - now;
                let retry_sender = sender.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(delay)).await;
                    let _ = retry_sender.send(job).await;
                });
                continue;
            }
            let job_id = job.id.clone();
            {
                let state = jobs_state();
                let mut guard = state.lock().expect("jobs state lock");
                let queue_state = queue_state_mut(&mut guard, &queue_name);
                if queue_state.inflight.contains(&job_id) {
                    continue;
                }
                queue_state.inflight.insert(job_id.clone());
            }
            let done = storage_get_string(&done_key(&queue_name, &job.id)).await;
            if done.is_some() {
                let _ = storage_delete(&job.storage_key).await;
                let state = jobs_state();
                let mut guard = state.lock().expect("jobs state lock");
                let queue_state = queue_state_mut(&mut guard, &queue_name);
                queue_state.inflight.remove(&job_id);
                unsafe { wr_rc_dec(job.payload) };
                continue;
            }
            let lease_key = lease_key(&queue_name, &job.id);
            if ha_enabled {
                let acquired =
                    lease::try_acquire_lease(&lease_key, &owner, lease_ttl_default).await;
                if !acquired {
                    let state = jobs_state();
                    let mut guard = state.lock().expect("jobs state lock");
                    let queue_state = queue_state_mut(&mut guard, &queue_name);
                    queue_state.inflight.remove(&job_id);
                    let retry_sender = sender.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        let _ = retry_sender.send(job).await;
                    });
                    continue;
                }
            }
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
                let _ = storage_delete(&job.storage_key).await;
                let _ =
                    storage_set_string_if_version(&done_key(&queue_name, &job.id), "1", None).await;
                if ha_enabled {
                    let _ = lease::release_lease(&lease_key, &owner).await;
                }
                let state = jobs_state();
                let mut guard = state.lock().expect("jobs state lock");
                let queue_state = queue_state_mut(&mut guard, &queue_name);
                queue_state.inflight.remove(&job_id);
                unsafe { wr_rc_dec(job.payload) };
                continue;
            }
            job.attempts += 1;
            if job.attempts > job.max_retries {
                let _ = storage_delete(&job.storage_key).await;
                let dlq_entry = StoredJob {
                    id: job.id.clone(),
                    payload: json_from_map(job.payload),
                    attempts: job.attempts,
                    max_retries: job.max_retries,
                    backoff_secs: job.backoff_secs,
                    enqueued_at: job.enqueued_at,
                    run_at: now_secs(),
                };
                let dlq_storage_key = dlq_key(&queue_name, &job.id);
                let _ = storage_set_json(&dlq_storage_key, &dlq_entry).await;
                if ha_enabled {
                    let _ = lease::release_lease(&lease_key, &owner).await;
                }
                let state = jobs_state();
                let mut guard = state.lock().expect("jobs state lock");
                let queue_state = queue_state_mut(&mut guard, &queue_name);
                queue_state.inflight.remove(&job_id);
                unsafe { wr_rc_dec(job.payload) };
                continue;
            }
            let retry_sender = sender.clone();
            let delay = job.backoff_secs * (job.attempts as u64);
            job.run_at = now_secs().saturating_add(delay);
            let stored = StoredJob {
                id: job.id.clone(),
                payload: json_from_map(job.payload),
                attempts: job.attempts,
                max_retries: job.max_retries,
                backoff_secs: job.backoff_secs,
                enqueued_at: job.enqueued_at,
                run_at: job.run_at,
            };
            let _ = storage_set_json(&job.storage_key, &stored).await;
            if ha_enabled {
                let _ = lease::renew_lease(&lease_key, &owner, delay.saturating_add(30)).await;
            }
            {
                let state = jobs_state();
                let mut guard = state.lock().expect("jobs state lock");
                let queue_state = queue_state_mut(&mut guard, &queue_name);
                queue_state.inflight.remove(&job_id);
            }
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
