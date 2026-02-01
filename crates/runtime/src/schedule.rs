use crate::actor::{
    actor_class_id, actor_send, actor_spawn, pending_await_async, pending_new, resolve_pending,
    runtime_spawn,
};
use crate::http::method_id_for;
use crate::lease;
use crate::metrics;
use crate::pubsub;
use crate::storage_helpers::{storage_get_string_with_version, storage_set_string_if_version};
use crate::value::Value;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
#[cfg(any(test, feature = "test-utils"))]
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Notify;

const SCHEDULE_KEY: &str = "schedule:entries";
const SCHEDULE_LEADER_LEASE: &str = "lease:scheduler:leader";
const LEASE_TTL_SECS: u64 = 8;
const TICK_SECS: u64 = 1;

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Clone, Serialize, Deserialize)]
struct ScheduleEntry {
    id: String,
    kind: String,
    expr: Option<String>,
    seconds: Option<i64>,
    timestamp: Option<i64>,
    interval_secs: u64,
    next_run: u64,
    class_id: u32,
    method_id: u32,
}

struct SchedulerState {
    started: AtomicBool,
    notify: Notify,
    stop: AtomicBool,
    lease_ttl_secs: u64,
}

static SCHEDULER_STATE: OnceLock<Arc<SchedulerState>> = OnceLock::new();
static PUBSUB_REGISTERED: OnceLock<()> = OnceLock::new();

#[cfg(any(test, feature = "test-utils"))]
tokio::task_local! {
    static SCHEDULER_STATE_OVERRIDE: Arc<SchedulerState>;
}

fn scheduler_state() -> Arc<SchedulerState> {
    #[cfg(any(test, feature = "test-utils"))]
    if let Ok(state) = SCHEDULER_STATE_OVERRIDE.try_with(Arc::clone) {
        return state;
    }
    SCHEDULER_STATE
        .get_or_init(|| {
            Arc::new(SchedulerState {
                started: AtomicBool::new(false),
                notify: Notify::new(),
                stop: AtomicBool::new(false),
                lease_ttl_secs: LEASE_TTL_SECS,
            })
        })
        .clone()
}

#[cfg(any(test, feature = "test-utils"))]
#[allow(dead_code)]
pub struct SchedulerStateHandle(Arc<SchedulerState>);

#[cfg(any(test, feature = "test-utils"))]
#[allow(dead_code)]
pub fn new_scheduler_state_for_test() -> SchedulerStateHandle {
    SchedulerStateHandle(Arc::new(SchedulerState {
        started: AtomicBool::new(false),
        notify: Notify::new(),
        stop: AtomicBool::new(false),
        lease_ttl_secs: LEASE_TTL_SECS,
    }))
}

#[cfg(any(test, feature = "test-utils"))]
#[allow(dead_code)]
pub fn new_scheduler_state_for_test_with_ttl(ttl_secs: u64) -> SchedulerStateHandle {
    SchedulerStateHandle(Arc::new(SchedulerState {
        started: AtomicBool::new(false),
        notify: Notify::new(),
        stop: AtomicBool::new(false),
        lease_ttl_secs: ttl_secs.max(1),
    }))
}

#[cfg(any(test, feature = "test-utils"))]
#[allow(dead_code)]
pub async fn stop_scheduler_for_test(state: &SchedulerStateHandle) {
    state.0.stop.store(true, Ordering::Release);
    state.0.notify.notify_one();
    let owner = lease::owner_id();
    let _ = lease::release_lease(SCHEDULE_LEADER_LEASE, &owner).await;
}

#[cfg(any(test, feature = "test-utils"))]
#[allow(dead_code)]
pub async fn with_scheduler_state_override<F, R>(state: &SchedulerStateHandle, fut: F) -> R
where
    F: Future<Output = R>,
{
    SCHEDULER_STATE_OVERRIDE.scope(state.0.clone(), fut).await
}

pub fn ensure_scheduler_started() {
    let state = scheduler_state();
    ensure_pubsub_registered();
    if state
        .started
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    runtime_spawn(async move {
        scheduler_loop(state).await;
    });
}

async fn load_entries_with_version() -> (Vec<ScheduleEntry>, Option<u64>) {
    match storage_get_string_with_version(SCHEDULE_KEY).await {
        Some((raw, version)) => {
            let entries = serde_json::from_str::<Vec<ScheduleEntry>>(&raw).unwrap_or_default();
            (entries, Some(version))
        }
        None => (Vec::new(), None),
    }
}

async fn store_entries_if_version(entries: &[ScheduleEntry], version: Option<u64>) -> bool {
    let Ok(raw) = serde_json::to_string(entries) else {
        return false;
    };
    storage_set_string_if_version(SCHEDULE_KEY, &raw, version).await
}

async fn append_schedule(entry: ScheduleEntry) {
    for _ in 0..6 {
        let (mut entries, version) = load_entries_with_version().await;
        entries.push(entry.clone());
        if store_entries_if_version(&entries, version).await {
            scheduler_state().notify.notify_one();
            #[cfg(feature = "metrics")]
            metrics::inc_sched_wakeup();
            pubsub::publish("schedule:wakeup", JsonValue::Null).await;
            return;
        }
        tokio::task::yield_now().await;
    }
}

fn ensure_pubsub_registered() {
    if PUBSUB_REGISTERED.set(()).is_err() {
        return;
    }
    runtime_spawn(async {
        pubsub::subscribe("schedule:wakeup", |_| async {
            scheduler_state().notify.notify_one();
        })
        .await;
    });
}

fn run_key(entry_id: &str, next_run: u64) -> String {
    format!("schedule:run:{entry_id}:{next_run}")
}

async fn claim_run(entry: &ScheduleEntry) -> bool {
    storage_set_string_if_version(&run_key(&entry.id, entry.next_run), "1", None).await
}

#[cfg(test)]
pub(crate) async fn claim_run_for_test(entry_id: &str, run_at: u64) -> bool {
    storage_set_string_if_version(&run_key(entry_id, run_at), "1", None).await
}

fn spawn_job(entry: &ScheduleEntry) {
    let class_id = entry.class_id as u64;
    let method_id = entry.method_id;
    runtime_spawn(async move {
        let handler = actor_spawn(class_id, Value::nil(), 1, 3, -1, -1, -1);
        let args = [];
        let pending = actor_send(handler, method_id, 0, args.as_ptr());
        let result = pending_await_async(pending).await;
        if pending.is_ptr() {
            unsafe { crate::wr_rc_dec(pending) };
        }
        if result.is_ptr() {
            unsafe { crate::wr_rc_dec(result) };
        }
        if handler.is_ptr() {
            unsafe { crate::wr_rc_dec(handler) };
        }
    });
}

async fn scheduler_loop(state: Arc<SchedulerState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(TICK_SECS));
    let owner = lease::owner_id();
    let mut is_leader = false;
    loop {
        if state.stop.load(Ordering::Acquire) {
            break;
        }
        tokio::select! {
            _ = interval.tick() => {}
            _ = state.notify.notified() => {}
        }
        if state.stop.load(Ordering::Acquire) {
            break;
        }
        if is_leader {
            if !lease::renew_lease(SCHEDULE_LEADER_LEASE, &owner, state.lease_ttl_secs).await {
                is_leader = false;
            }
        } else {
            is_leader = lease::try_acquire_lease(
                SCHEDULE_LEADER_LEASE,
                &owner,
                state.lease_ttl_secs,
            )
                .await;
        }
        if !is_leader {
            continue;
        }
        let now = now_secs();
        let (entries, version) = load_entries_with_version().await;
        if entries.is_empty() {
            continue;
        }
        let mut changed = false;
        let mut next_entries = Vec::with_capacity(entries.len());
        for mut entry in entries {
            if entry.next_run <= now {
                if claim_run(&entry).await {
                    spawn_job(&entry);
                }
                if entry.kind == "at" {
                    changed = true;
                    continue;
                }
                let interval_secs = entry.interval_secs.max(1);
                entry.next_run = now.saturating_add(interval_secs);
                changed = true;
            }
            next_entries.push(entry);
        }
        if changed {
            let _ = store_entries_if_version(&next_entries, version).await;
        }
    }
}

fn schedule_entry(
    kind: &str,
    expr: Option<String>,
    seconds: Option<u64>,
    timestamp: Option<u64>,
    class_id: u32,
    method_id: u32,
) -> ScheduleEntry {
    let now = now_secs();
    let interval_secs = seconds.unwrap_or(0).max(1);
    let next_run = if kind == "at" {
        timestamp.unwrap_or(now)
    } else {
        now
    };
    ScheduleEntry {
        id: uuid::Uuid::new_v4().to_string(),
        kind: kind.to_string(),
        expr,
        seconds: seconds.map(|v| v as i64),
        timestamp: timestamp.map(|v| v as i64),
        interval_secs,
        next_run,
        class_id,
        method_id,
    }
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
    ensure_scheduler_started();
    runtime_spawn(async move {
        let entry = schedule_entry("every", None, Some(secs), None, class_id, method_id);
        append_schedule(entry).await;
        resolve_pending(state, Value::from_bool(true));
    });
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
    let ts = ts.max(0) as u64;
    ensure_scheduler_started();
    runtime_spawn(async move {
        let entry = schedule_entry("at", None, None, Some(ts), class_id, method_id);
        append_schedule(entry).await;
        resolve_pending(state, Value::from_bool(true));
    });
    pending
}

pub fn schedule_cron(storage: Value, expr: Value, job: Value) -> Value {
    let (pending, state) = pending_new();
    if storage.is_nil() {
        resolve_pending(state, Value::from_bool(false));
        return pending;
    }
    let Some(expr) = crate::storage_helpers::value_to_string(expr) else {
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
    ensure_scheduler_started();
    runtime_spawn(async move {
        let entry =
            schedule_entry("cron", Some(expr), Some(interval_secs), None, class_id, method_id);
        append_schedule(entry).await;
        resolve_pending(state, Value::from_bool(true));
    });
    pending
}
