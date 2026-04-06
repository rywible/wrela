//! Runtime metrics. Scheduler/web metrics retained for JSON dump compatibility (always 0).
#![allow(dead_code)]

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

pub const METRIC_ALLOC_STRING: u32 = 1;
pub const METRIC_ALLOC_LIST: u32 = 2;
pub const METRIC_ALLOC_MAP: u32 = 3;
pub const METRIC_ALLOC_BYTES: u32 = 4;
pub const METRIC_ALLOC_RESULT: u32 = 5;
pub const METRIC_ALLOC_PENDING: u32 = 6;
pub const METRIC_RC_INC: u32 = 7;
pub const METRIC_RC_DEC: u32 = 8;
pub const METRIC_MESSAGES_DROPPED_PAUSED: u32 = 9;
pub const METRIC_MESSAGES_DROPPED: u32 = 10;
pub const METRIC_MESSAGES_SENT: u32 = 11;
pub const METRIC_PENDING_DROPPED: u32 = 12;
pub const METRIC_PENDING_RESOLVED: u32 = 13;
pub const METRIC_MAILBOX_ENQUEUE_OK: u32 = 14;
pub const METRIC_MAILBOX_ENQUEUE_FAIL: u32 = 15;
pub const METRIC_MAILBOX_DEQUEUE: u32 = 16;
pub const METRIC_POOL_QUEUE_FULL: u32 = 17;
pub const METRIC_POOL_ENQUEUE_AFTER_RETIRE: u32 = 18;
pub const METRIC_SCHED_DISPATCHED: u32 = 19;
pub const METRIC_SCHED_SKIPPED_NO_CREDIT: u32 = 20;
pub const METRIC_ABI_TYPED_LANE: u32 = 21;
pub const METRIC_ABI_BOXED_LANE: u32 = 22;
pub const METRIC_SCHED_PROFILE_SWITCH: u32 = 23;
pub const METRIC_SCHED_STARVATION_VIOLATION: u32 = 24;
pub const METRIC_SCHED_CROSS_SHARD_MIGRATION: u32 = 25;
pub const METRIC_QUEUE_CAS_RETRY: u32 = 26;
pub const METRIC_MAILBOX_WAKE_COALESCED: u32 = 27;
pub const METRIC_MAILBOX_RESCUE_WAKE: u32 = 28;
pub const METRIC_SCHED_LOCAL_DISPATCH: u32 = 29;
pub const METRIC_SCHED_GLOBAL_DISPATCH: u32 = 30;
pub const METRIC_SCHED_PLAN_RECOMPUTE: u32 = 31;
pub const METRIC_SCHED_STEAL_ATTEMPTS: u32 = 32;
pub const METRIC_SCHED_STEAL_SUCCESS: u32 = 33;
pub const METRIC_SCHED_MIGRATION_BLOCKED_HYSTERESIS: u32 = 34;
pub const METRIC_SCHED_MIGRATION_BLOCKED_COOLDOWN: u32 = 35;
// Workqueue / mailbox attribution metrics (low overhead counters).
pub const METRIC_MESSAGE_BUILD_NOARGS: u32 = 36;
pub const METRIC_MESSAGE_BUILD_ARGS: u32 = 37;
pub const METRIC_MESSAGE_INSTANCE_RC_SKIPPED: u32 = 38;
pub const METRIC_ACTOR_ARENA_LOCK: u32 = 39;
pub const METRIC_MAILBOX_BATCH_RESERVE_OK: u32 = 40;
pub const METRIC_MAILBOX_BATCH_RESERVE_FAIL: u32 = 41;
pub const METRIC_MESSAGE_INSTANCE_IS_ARENA: u32 = 42;
pub const METRIC_ACTOR_SPAWN_INSTANCE_IS_PTR: u32 = 43;
pub const METRIC_ACTOR_SPAWN_INSTANCE_NOT_PTR: u32 = 44;
pub const METRIC_ACTOR_SPAWN_INSTANCE_PROMOTED: u32 = 45;
pub const METRIC_ACTOR_METHOD_PANIC: u32 = 46;
pub const METRIC_ACTOR_METHOD_MISSING: u32 = 47;
pub const METRIC_WEB_OUTBOUND_QUEUE_ENQUEUED_BYTES: u32 = 48;
pub const METRIC_WEB_OUTBOUND_QUEUE_PENDING_BYTES: u32 = 49;
pub const METRIC_WEB_FLUSH_ATTEMPTS: u32 = 50;
pub const METRIC_WEB_FLUSH_WOULD_BLOCK: u32 = 51;
pub const METRIC_WEB_WRITEV_CALLS: u32 = 52;
pub const METRIC_WEB_WRITEV_BYTES: u32 = 53;
pub const METRIC_WEB_SENDFILE_CALLS: u32 = 54;
pub const METRIC_WEB_SENDFILE_BYTES: u32 = 55;
pub const METRIC_WEB_SENDFILE_FALLBACK: u32 = 56;
pub const METRIC_REACTOR_BATCH_DRAIN_TOTAL: u32 = 57;
pub const METRIC_REACTOR_BATCH_DRAIN_SAMPLES: u32 = 58;
pub const METRIC_SCHED_READY_OVERFLOW_FALLBACK: u32 = 59;
pub const METRIC_SCENE_TRACE: u32 = 60;
pub const METRIC_FIELD_SAMPLE: u32 = 61;
pub const METRIC_SCENE_TRACE_SUPPORT_PRUNED_BRANCH: u32 = 62;
pub const METRIC_SCENE_TRACE_CANDIDATE_BRANCH: u32 = 63;
pub const METRIC_SCENE_TRACE_EXACT_PATH: u32 = 64;
pub const METRIC_SCENE_TRACE_CONSERVATIVE_PATH: u32 = 65;
pub const METRIC_SCENE_TRACE_HIT_COUNT: u32 = 66;
pub const METRIC_SCENE_TRACE_HIT_STEPS_TOTAL: u32 = 67;
pub const METRIC_SCENE_TRACE_HIT_FIELD_SAMPLES_TOTAL: u32 = 68;
pub const METRIC_SCENE_TRACE_STEPS_LE_1: u32 = 69;
pub const METRIC_SCENE_TRACE_STEPS_LE_4: u32 = 70;
pub const METRIC_SCENE_TRACE_STEPS_LE_8: u32 = 71;
pub const METRIC_SCENE_TRACE_STEPS_LE_16: u32 = 72;
pub const METRIC_SCENE_TRACE_STEPS_GT_16: u32 = 73;
pub const METRIC_SCENE_TRACE_BLEND_COST: u32 = 74;
pub const METRIC_SCENE_TRACE_DEFORMATION_COST: u32 = 75;
const METRIC_COUNT: usize = 96;
const LATENCY_BUCKETS: [u64; 12] = [
    1_000,
    2_000,
    5_000,
    10_000,
    20_000,
    50_000,
    100_000,
    250_000,
    500_000,
    1_000_000,
    5_000_000,
    u64::MAX,
];
static METRICS: [AtomicU64; METRIC_COUNT] = [const { AtomicU64::new(0) }; METRIC_COUNT];
static MAILBOX_HIGH_WATER: AtomicU64 = AtomicU64::new(0);
static QUEUE_ENQUEUE_LATENCY_HIST: [AtomicU64; LATENCY_BUCKETS.len()] =
    [const { AtomicU64::new(0) }; LATENCY_BUCKETS.len()];
static QUEUE_DEQUEUE_LATENCY_HIST: [AtomicU64; LATENCY_BUCKETS.len()] =
    [const { AtomicU64::new(0) }; LATENCY_BUCKETS.len()];
static QUEUE_AGE_HIST: [AtomicU64; LATENCY_BUCKETS.len()] =
    [const { AtomicU64::new(0) }; LATENCY_BUCKETS.len()];
static SCHED_DISPATCH_LOOP_NS_HIST: [AtomicU64; LATENCY_BUCKETS.len()] =
    [const { AtomicU64::new(0) }; LATENCY_BUCKETS.len()];
static WEB_OUTBOUND_QUEUE_AGE_HIST: [AtomicU64; LATENCY_BUCKETS.len()] =
    [const { AtomicU64::new(0) }; LATENCY_BUCKETS.len()];
static BURST_DRAIN_TOTAL: AtomicU64 = AtomicU64::new(0);
static BURST_DRAIN_SAMPLES: AtomicU64 = AtomicU64::new(0);
static SCHED_SAMPLE_COUNTER: AtomicU64 = AtomicU64::new(0);
static ACTOR_SPAWN_INSTANCE_TYPE_ID: AtomicU64 = AtomicU64::new(0);
static METRICS_DUMP_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
static DUMP_HOOK_INSTALLED: OnceLock<()> = OnceLock::new();
static METRICS_ENABLED: OnceLock<bool> = OnceLock::new();
static FUNCTION_COVERAGE_ENABLED: OnceLock<bool> = OnceLock::new();
static FUNCTION_COVERAGE: OnceLock<Mutex<HashMap<u64, u64>>> = OnceLock::new();

#[cfg(test)]
pub fn test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[inline(always)]
fn enabled() -> bool {
    *METRICS_ENABLED.get_or_init(
        || match std::env::var("WRELA_RUNTIME_METRICS").ok().as_deref() {
            Some("0") => false,
            _ => true,
        },
    )
}

#[inline(always)]
pub fn is_enabled() -> bool {
    enabled()
}

fn function_coverage() -> &'static Mutex<HashMap<u64, u64>> {
    FUNCTION_COVERAGE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[inline(always)]
fn function_coverage_enabled() -> bool {
    *FUNCTION_COVERAGE_ENABLED.get_or_init(|| metrics_dump_path().is_some())
}

#[inline(always)]
fn bump(id: u32) {
    if !enabled() {
        return;
    }
    if let Some(metric) = METRICS.get(id as usize) {
        metric.fetch_add(1, Ordering::Relaxed);
    }
}

#[inline(always)]
fn bump_by(id: u32, value: u64) {
    if !enabled() {
        return;
    }
    if value == 0 {
        return;
    }
    if let Some(metric) = METRICS.get(id as usize) {
        metric.fetch_add(value, Ordering::Relaxed);
    }
}

#[inline(always)]
fn bump_signed(id: u32, delta: i64) {
    if !enabled() || delta == 0 {
        return;
    }
    let Some(metric) = METRICS.get(id as usize) else {
        return;
    };
    if delta > 0 {
        metric.fetch_add(delta as u64, Ordering::Relaxed);
        return;
    }
    let decrement = delta.unsigned_abs();
    let mut current = metric.load(Ordering::Relaxed);
    loop {
        let next = current.saturating_sub(decrement);
        match metric.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(actual) => current = actual,
        }
    }
}

pub fn get(id: u32) -> u64 {
    METRICS
        .get(id as usize)
        .map(|metric| metric.load(Ordering::Relaxed))
        .unwrap_or(0)
}

#[cfg(test)]
pub fn metrics_get_raw(id: u32) -> u64 {
    get(id)
}

pub fn reset() {
    for metric in METRICS.iter() {
        metric.store(0, Ordering::Relaxed);
    }
    MAILBOX_HIGH_WATER.store(0, Ordering::Relaxed);
    for bucket in QUEUE_ENQUEUE_LATENCY_HIST.iter() {
        bucket.store(0, Ordering::Relaxed);
    }
    for bucket in QUEUE_DEQUEUE_LATENCY_HIST.iter() {
        bucket.store(0, Ordering::Relaxed);
    }
    for bucket in QUEUE_AGE_HIST.iter() {
        bucket.store(0, Ordering::Relaxed);
    }
    for bucket in SCHED_DISPATCH_LOOP_NS_HIST.iter() {
        bucket.store(0, Ordering::Relaxed);
    }
    for bucket in WEB_OUTBOUND_QUEUE_AGE_HIST.iter() {
        bucket.store(0, Ordering::Relaxed);
    }
    BURST_DRAIN_TOTAL.store(0, Ordering::Relaxed);
    BURST_DRAIN_SAMPLES.store(0, Ordering::Relaxed);
    SCHED_SAMPLE_COUNTER.store(0, Ordering::Relaxed);
    function_coverage()
        .lock()
        .expect("function coverage lock")
        .clear();
}

pub fn install_dump_hook() {
    if metrics_dump_path().is_none() {
        return;
    }
    if DUMP_HOOK_INSTALLED.set(()).is_err() {
        return;
    }
    unsafe {
        libc::atexit(dump_at_exit);
    }
}

extern "C" fn dump_at_exit() {
    maybe_dump_metrics();
}

fn metrics_dump_path() -> Option<PathBuf> {
    METRICS_DUMP_PATH
        .get_or_init(|| env::var("WRELA_METRICS_PATH").ok().map(PathBuf::from))
        .clone()
}

fn observe_histogram(hist: &[AtomicU64; LATENCY_BUCKETS.len()], value_ns: u64) {
    for (idx, bound) in LATENCY_BUCKETS.iter().enumerate() {
        if value_ns <= *bound {
            hist[idx].fetch_add(1, Ordering::Relaxed);
            return;
        }
    }
    if let Some(last) = hist.last() {
        last.fetch_add(1, Ordering::Relaxed);
    }
}

fn histogram_pctl_ns(hist: &[AtomicU64; LATENCY_BUCKETS.len()], pct: f64) -> u64 {
    let total = hist
        .iter()
        .map(|value| value.load(Ordering::Relaxed))
        .sum::<u64>();
    if total == 0 {
        return 0;
    }
    let rank = ((total as f64) * pct).ceil() as u64;
    let rank = rank.max(1);
    let mut seen = 0u64;
    for (idx, count) in hist.iter().enumerate() {
        seen = seen.saturating_add(count.load(Ordering::Relaxed));
        if seen >= rank {
            return LATENCY_BUCKETS[idx];
        }
    }
    *LATENCY_BUCKETS.last().unwrap_or(&0)
}

fn maybe_dump_metrics() {
    let Some(path) = metrics_dump_path() else {
        return;
    };
    let enqueue_p99 = histogram_pctl_ns(&QUEUE_ENQUEUE_LATENCY_HIST, 0.99);
    let dequeue_p99 = histogram_pctl_ns(&QUEUE_DEQUEUE_LATENCY_HIST, 0.99);
    let queue_age_p99 = histogram_pctl_ns(&QUEUE_AGE_HIST, 0.99);
    let sched_dispatch_loop_p99 = histogram_pctl_ns(&SCHED_DISPATCH_LOOP_NS_HIST, 0.99);
    let burst_samples = BURST_DRAIN_SAMPLES.load(Ordering::Relaxed);
    let burst_total = BURST_DRAIN_TOTAL.load(Ordering::Relaxed);
    let burst_avg = if burst_samples == 0 {
        0.0
    } else {
        burst_total as f64 / burst_samples as f64
    };
    let function_coverage = {
        let mut entries = function_coverage()
            .lock()
            .expect("function coverage lock")
            .iter()
            .map(|(function_id, hits)| (*function_id, *hits))
            .collect::<Vec<_>>();
        entries.sort_unstable_by_key(|(function_id, _)| *function_id);
        let body = entries
            .into_iter()
            .map(|(function_id, hits)| format!("\"{function_id}\":{hits}"))
            .collect::<Vec<_>>()
            .join(",");
        format!("{{{body}}}")
    };
    let data = format!(
        "{{\"messages_sent\":{},\"messages_dropped\":{},\"pending_resolved\":{},\"pending_dropped\":{},\"mailbox_high_water\":{},\"rc_inc\":{},\"rc_dec\":{},\"alloc_list\":{},\"alloc_map\":{},\"alloc_string\":{},\"alloc_bytes\":{},\"alloc_result\":{},\"alloc_pending\":{},\"mailbox_enqueue_ok\":{},\"mailbox_enqueue_fail\":{},\"mailbox_dequeue\":{},\"sched_dispatched\":{},\"sched_skipped_no_credit\":{},\"sched_profile_switch\":{},\"sched_starvation_violation\":{},\"sched_cross_shard_migration\":{},\"abi_typed_lane\":{},\"abi_boxed_lane\":{},\"queue_cas_retry_total\":{},\"mailbox_wake_coalesced_count\":{},\"mailbox_rescue_wake_count\":{},\"sched_local_dispatch_count\":{},\"sched_global_dispatch_count\":{},\"sched_plan_recompute_count\":{},\"sched_steal_attempts\":{},\"sched_steal_success\":{},\"sched_migration_blocked_hysteresis\":{},\"sched_migration_blocked_cooldown\":{},\"sched_ready_overflow_fallback\":{},\"scene_trace\":{},\"field_sample\":{},\"scene_trace_blend_cost\":{},\"scene_trace_deformation_cost\":{},\"message_build_noargs_count\":{},\"message_build_args_count\":{},\"message_instance_rc_skipped_count\":{},\"message_instance_is_arena_count\":{},\"actor_spawn_instance_is_ptr_count\":{},\"actor_spawn_instance_not_ptr_count\":{},\"actor_spawn_instance_promoted_count\":{},\"actor_spawn_instance_type_id\":{},\"actor_method_panic_count\":{},\"actor_method_missing_count\":{},\"actor_arena_lock_count\":{},\"mailbox_batch_reserve_success\":{},\"mailbox_batch_reserve_failed\":{},\"queue_enqueue_p99_ns\":{},\"queue_dequeue_p99_ns\":{},\"queue_age_p99_ns\":{},\"sched_dispatch_loop_ns_p99\":{},\"queue_burst_drain_avg\":{:.2},\"function_coverage\":{}}}",
        get(METRIC_MESSAGES_SENT),
        get(METRIC_MESSAGES_DROPPED),
        get(METRIC_PENDING_RESOLVED),
        get(METRIC_PENDING_DROPPED),
        MAILBOX_HIGH_WATER.load(Ordering::Relaxed),
        get(METRIC_RC_INC),
        get(METRIC_RC_DEC),
        get(METRIC_ALLOC_LIST),
        get(METRIC_ALLOC_MAP),
        get(METRIC_ALLOC_STRING),
        get(METRIC_ALLOC_BYTES),
        get(METRIC_ALLOC_RESULT),
        get(METRIC_ALLOC_PENDING),
        get(METRIC_MAILBOX_ENQUEUE_OK),
        get(METRIC_MAILBOX_ENQUEUE_FAIL),
        get(METRIC_MAILBOX_DEQUEUE),
        get(METRIC_SCHED_DISPATCHED),
        get(METRIC_SCHED_SKIPPED_NO_CREDIT),
        get(METRIC_SCHED_PROFILE_SWITCH),
        get(METRIC_SCHED_STARVATION_VIOLATION),
        get(METRIC_SCHED_CROSS_SHARD_MIGRATION),
        get(METRIC_ABI_TYPED_LANE),
        get(METRIC_ABI_BOXED_LANE),
        get(METRIC_QUEUE_CAS_RETRY),
        get(METRIC_MAILBOX_WAKE_COALESCED),
        get(METRIC_MAILBOX_RESCUE_WAKE),
        get(METRIC_SCHED_LOCAL_DISPATCH),
        get(METRIC_SCHED_GLOBAL_DISPATCH),
        get(METRIC_SCHED_PLAN_RECOMPUTE),
        get(METRIC_SCHED_STEAL_ATTEMPTS),
        get(METRIC_SCHED_STEAL_SUCCESS),
        get(METRIC_SCHED_MIGRATION_BLOCKED_HYSTERESIS),
        get(METRIC_SCHED_MIGRATION_BLOCKED_COOLDOWN),
        get(METRIC_SCHED_READY_OVERFLOW_FALLBACK),
        get(METRIC_SCENE_TRACE),
        get(METRIC_FIELD_SAMPLE),
        get(METRIC_SCENE_TRACE_BLEND_COST),
        get(METRIC_SCENE_TRACE_DEFORMATION_COST),
        get(METRIC_MESSAGE_BUILD_NOARGS),
        get(METRIC_MESSAGE_BUILD_ARGS),
        get(METRIC_MESSAGE_INSTANCE_RC_SKIPPED),
        get(METRIC_MESSAGE_INSTANCE_IS_ARENA),
        get(METRIC_ACTOR_SPAWN_INSTANCE_IS_PTR),
        get(METRIC_ACTOR_SPAWN_INSTANCE_NOT_PTR),
        get(METRIC_ACTOR_SPAWN_INSTANCE_PROMOTED),
        ACTOR_SPAWN_INSTANCE_TYPE_ID.load(Ordering::Relaxed),
        get(METRIC_ACTOR_METHOD_PANIC),
        get(METRIC_ACTOR_METHOD_MISSING),
        get(METRIC_ACTOR_ARENA_LOCK),
        get(METRIC_MAILBOX_BATCH_RESERVE_OK),
        get(METRIC_MAILBOX_BATCH_RESERVE_FAIL),
        enqueue_p99,
        dequeue_p99,
        queue_age_p99,
        sched_dispatch_loop_p99,
        burst_avg,
        function_coverage,
    );
    let _ = fs::write(path, data.as_bytes());
}

pub fn coverage_hit(function_id: u64) {
    if !enabled() || !function_coverage_enabled() {
        return;
    }
    let mut coverage = function_coverage().lock().expect("function coverage lock");
    *coverage.entry(function_id).or_insert(0) += 1;
}

pub fn inc_alloc_string() {
    bump(METRIC_ALLOC_STRING)
}
pub fn inc_alloc_list() {
    bump(METRIC_ALLOC_LIST)
}
pub fn inc_alloc_map() {
    bump(METRIC_ALLOC_MAP)
}
pub fn inc_alloc_bytes() {
    bump(METRIC_ALLOC_BYTES)
}
pub fn inc_alloc_result() {
    bump(METRIC_ALLOC_RESULT)
}
pub fn inc_alloc_pending() {
    bump(METRIC_ALLOC_PENDING)
}
pub fn inc_rc_inc() {
    bump(METRIC_RC_INC)
}
pub fn inc_rc_dec() {
    bump(METRIC_RC_DEC)
}
pub fn inc_messages_dropped_paused() {
    bump(METRIC_MESSAGES_DROPPED_PAUSED)
}
pub fn inc_messages_dropped() {
    bump(METRIC_MESSAGES_DROPPED)
}
pub fn inc_messages_sent() {
    bump(METRIC_MESSAGES_SENT)
}
pub fn inc_messages_sent_n(value: u64) {
    bump_by(METRIC_MESSAGES_SENT, value)
}
pub fn inc_pending_dropped() {
    bump(METRIC_PENDING_DROPPED)
}
pub fn inc_pending_resolved() {
    bump(METRIC_PENDING_RESOLVED)
}
pub fn inc_mailbox_enqueue_ok() {
    bump(METRIC_MAILBOX_ENQUEUE_OK)
}
pub fn inc_mailbox_enqueue_ok_n(value: u64) {
    bump_by(METRIC_MAILBOX_ENQUEUE_OK, value)
}
pub fn inc_mailbox_enqueue_fail() {
    bump(METRIC_MAILBOX_ENQUEUE_FAIL)
}
pub fn inc_mailbox_dequeue_n(value: u64) {
    bump_by(METRIC_MAILBOX_DEQUEUE, value)
}
pub fn inc_pool_queue_full() {
    bump(METRIC_POOL_QUEUE_FULL)
}
pub fn inc_pool_enqueue_after_retire() {
    bump(METRIC_POOL_ENQUEUE_AFTER_RETIRE)
}
pub fn inc_sched_dispatched() {
    bump(METRIC_SCHED_DISPATCHED)
}
pub fn inc_sched_skipped_no_credit() {
    bump(METRIC_SCHED_SKIPPED_NO_CREDIT)
}
pub fn inc_sched_profile_switch() {
    bump(METRIC_SCHED_PROFILE_SWITCH)
}
pub fn inc_sched_starvation_violation() {
    bump(METRIC_SCHED_STARVATION_VIOLATION)
}
pub fn inc_sched_cross_shard_migration() {
    bump(METRIC_SCHED_CROSS_SHARD_MIGRATION)
}
pub fn inc_queue_cas_retry_n(value: u64) {
    bump_by(METRIC_QUEUE_CAS_RETRY, value)
}
pub fn inc_mailbox_wake_coalesced() {
    bump(METRIC_MAILBOX_WAKE_COALESCED)
}
pub fn inc_mailbox_rescue_wake() {
    bump(METRIC_MAILBOX_RESCUE_WAKE)
}
pub fn inc_sched_local_dispatch() {
    bump(METRIC_SCHED_LOCAL_DISPATCH)
}
pub fn inc_sched_global_dispatch() {
    bump(METRIC_SCHED_GLOBAL_DISPATCH)
}
pub fn inc_sched_plan_recompute() {
    bump(METRIC_SCHED_PLAN_RECOMPUTE)
}
pub fn inc_sched_steal_attempt() {
    bump(METRIC_SCHED_STEAL_ATTEMPTS)
}
pub fn inc_sched_steal_success() {
    bump(METRIC_SCHED_STEAL_SUCCESS)
}
pub fn inc_sched_migration_blocked_hysteresis() {
    bump(METRIC_SCHED_MIGRATION_BLOCKED_HYSTERESIS)
}
pub fn inc_sched_migration_blocked_cooldown() {
    bump(METRIC_SCHED_MIGRATION_BLOCKED_COOLDOWN)
}
pub fn inc_sched_ready_overflow_fallback() {
    bump(METRIC_SCHED_READY_OVERFLOW_FALLBACK)
}
pub fn inc_scene_trace() {
    bump(METRIC_SCENE_TRACE)
}
pub fn inc_field_sample() {
    bump(METRIC_FIELD_SAMPLE)
}
pub fn inc_scene_trace_support_pruned_branch() {
    bump(METRIC_SCENE_TRACE_SUPPORT_PRUNED_BRANCH)
}
pub fn inc_scene_trace_candidate_branch() {
    bump(METRIC_SCENE_TRACE_CANDIDATE_BRANCH)
}
pub fn inc_scene_trace_exact_path() {
    bump(METRIC_SCENE_TRACE_EXACT_PATH)
}
pub fn inc_scene_trace_conservative_path() {
    bump(METRIC_SCENE_TRACE_CONSERVATIVE_PATH)
}
pub fn inc_scene_trace_hit(steps: u64, field_samples: u64) {
    bump(METRIC_SCENE_TRACE_HIT_COUNT);
    bump_by(METRIC_SCENE_TRACE_HIT_STEPS_TOTAL, steps);
    bump_by(METRIC_SCENE_TRACE_HIT_FIELD_SAMPLES_TOTAL, field_samples);
    bump(scene_trace_steps_bucket_id(steps));
}
pub fn inc_scene_trace_blend_cost() {
    bump(METRIC_SCENE_TRACE_BLEND_COST)
}
pub fn inc_scene_trace_deformation_cost() {
    bump(METRIC_SCENE_TRACE_DEFORMATION_COST)
}
pub fn inc_message_build_noargs() {
    bump(METRIC_MESSAGE_BUILD_NOARGS)
}
pub fn inc_message_build_args() {
    bump(METRIC_MESSAGE_BUILD_ARGS)
}
#[allow(dead_code)]
pub fn inc_message_instance_rc_skipped() {
    bump(METRIC_MESSAGE_INSTANCE_RC_SKIPPED)
}
#[allow(dead_code)]
pub fn inc_actor_arena_lock() {
    bump(METRIC_ACTOR_ARENA_LOCK)
}
pub fn inc_mailbox_batch_reserve_ok() {
    bump(METRIC_MAILBOX_BATCH_RESERVE_OK)
}
pub fn inc_mailbox_batch_reserve_fail() {
    bump(METRIC_MAILBOX_BATCH_RESERVE_FAIL)
}
pub fn inc_message_instance_is_arena() {
    bump(METRIC_MESSAGE_INSTANCE_IS_ARENA)
}
pub fn inc_actor_spawn_instance_is_ptr() {
    bump(METRIC_ACTOR_SPAWN_INSTANCE_IS_PTR)
}
pub fn inc_actor_spawn_instance_not_ptr() {
    bump(METRIC_ACTOR_SPAWN_INSTANCE_NOT_PTR)
}
pub fn inc_actor_spawn_instance_promoted() {
    bump(METRIC_ACTOR_SPAWN_INSTANCE_PROMOTED)
}
pub fn set_actor_spawn_instance_type_id(type_id: u64) {
    ACTOR_SPAWN_INSTANCE_TYPE_ID.store(type_id, Ordering::Relaxed);
}
pub fn inc_actor_method_panic() {
    bump(METRIC_ACTOR_METHOD_PANIC)
}
pub fn inc_actor_method_missing() {
    bump(METRIC_ACTOR_METHOD_MISSING)
}
pub fn observe_queue_enqueue_latency_ns(value_ns: u64) {
    if enabled() {
        observe_histogram(&QUEUE_ENQUEUE_LATENCY_HIST, value_ns);
    }
}
pub fn observe_queue_dequeue_latency_ns(value_ns: u64) {
    if enabled() {
        observe_histogram(&QUEUE_DEQUEUE_LATENCY_HIST, value_ns);
    }
}
pub fn observe_queue_age_ns(value_ns: u64) {
    if enabled() {
        observe_histogram(&QUEUE_AGE_HIST, value_ns);
    }
}
pub fn observe_queue_burst_drain(batch_size: u64) {
    if !enabled() {
        return;
    }
    BURST_DRAIN_TOTAL.fetch_add(batch_size, Ordering::Relaxed);
    BURST_DRAIN_SAMPLES.fetch_add(1, Ordering::Relaxed);
}
pub fn observe_sched_dispatch_loop_ns(value_ns: u64) {
    if enabled() {
        observe_histogram(&SCHED_DISPATCH_LOOP_NS_HIST, value_ns);
    }
}
pub fn observe_web_outbound_queue_age_ns(value_ns: u64) {
    if enabled() {
        observe_histogram(&WEB_OUTBOUND_QUEUE_AGE_HIST, value_ns);
    }
}
pub fn inc_web_outbound_queue_enqueued_bytes_n(value: u64) {
    bump_by(METRIC_WEB_OUTBOUND_QUEUE_ENQUEUED_BYTES, value)
}
pub fn inc_web_outbound_queue_pending_bytes_n(delta: i64) {
    bump_signed(METRIC_WEB_OUTBOUND_QUEUE_PENDING_BYTES, delta)
}
pub fn inc_web_flush_attempts() {
    bump(METRIC_WEB_FLUSH_ATTEMPTS)
}
pub fn inc_web_flush_would_block() {
    bump(METRIC_WEB_FLUSH_WOULD_BLOCK)
}
pub fn inc_web_writev_calls() {
    bump(METRIC_WEB_WRITEV_CALLS)
}
pub fn inc_web_writev_bytes_n(value: u64) {
    bump_by(METRIC_WEB_WRITEV_BYTES, value)
}
#[allow(dead_code)]
pub fn inc_web_sendfile_calls() {
    bump(METRIC_WEB_SENDFILE_CALLS)
}
pub fn inc_web_sendfile_bytes_n(value: u64) {
    bump_by(METRIC_WEB_SENDFILE_BYTES, value)
}
pub fn inc_web_sendfile_fallback() {
    bump(METRIC_WEB_SENDFILE_FALLBACK)
}
pub fn observe_reactor_batch_drain(batch_size: u64) {
    bump_by(METRIC_REACTOR_BATCH_DRAIN_TOTAL, batch_size);
    bump(METRIC_REACTOR_BATCH_DRAIN_SAMPLES);
}
pub fn should_sample_scheduler(sample_rate: u32) -> bool {
    if sample_rate <= 1 {
        return true;
    }
    let n = SCHED_SAMPLE_COUNTER.fetch_add(1, Ordering::Relaxed);
    n.is_multiple_of(sample_rate as u64)
}
#[cfg(test)]
pub fn inc_abi_typed_lane() {
    bump(METRIC_ABI_TYPED_LANE)
}
#[cfg(test)]
pub fn inc_abi_boxed_lane() {
    bump(METRIC_ABI_BOXED_LANE)
}

pub fn update_mailbox_high_water(len: usize) {
    if !enabled() {
        return;
    }
    let len = len as u64;
    let mut current = MAILBOX_HIGH_WATER.load(Ordering::Relaxed);
    while len > current {
        match MAILBOX_HIGH_WATER.compare_exchange_weak(
            current,
            len,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(actual) => current = actual,
        }
    }
}

fn scene_trace_steps_bucket_id(steps: u64) -> u32 {
    match steps {
        0 | 1 => METRIC_SCENE_TRACE_STEPS_LE_1,
        2..=4 => METRIC_SCENE_TRACE_STEPS_LE_4,
        5..=8 => METRIC_SCENE_TRACE_STEPS_LE_8,
        9..=16 => METRIC_SCENE_TRACE_STEPS_LE_16,
        _ => METRIC_SCENE_TRACE_STEPS_GT_16,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sched_ready_overflow_metric_tracks_and_resets() {
        let _guard = test_lock().lock().expect("metrics test lock");
        if !is_enabled() {
            return;
        }
        reset();
        assert_eq!(metrics_get_raw(METRIC_SCHED_READY_OVERFLOW_FALLBACK), 0);
        inc_sched_ready_overflow_fallback();
        assert_eq!(metrics_get_raw(METRIC_SCHED_READY_OVERFLOW_FALLBACK), 1);
        reset();
        assert_eq!(metrics_get_raw(METRIC_SCHED_READY_OVERFLOW_FALLBACK), 0);
    }

    #[test]
    fn scene_trace_and_field_sample_metrics_track_and_reset() {
        let _guard = test_lock().lock().expect("metrics test lock");
        if !is_enabled() {
            return;
        }
        reset();
        assert_eq!(metrics_get_raw(METRIC_SCENE_TRACE), 0);
        assert_eq!(metrics_get_raw(METRIC_FIELD_SAMPLE), 0);
        inc_scene_trace();
        inc_scene_trace();
        inc_field_sample();
        assert_eq!(metrics_get_raw(METRIC_SCENE_TRACE), 2);
        assert_eq!(metrics_get_raw(METRIC_FIELD_SAMPLE), 1);
        reset();
        assert_eq!(metrics_get_raw(METRIC_SCENE_TRACE), 0);
        assert_eq!(metrics_get_raw(METRIC_FIELD_SAMPLE), 0);
    }

    #[test]
    fn scene_trace_policy_metrics_track_and_reset() {
        let _guard = test_lock().lock().expect("metrics test lock");
        if !is_enabled() {
            return;
        }
        reset();
        inc_scene_trace_support_pruned_branch();
        inc_scene_trace_candidate_branch();
        inc_scene_trace_candidate_branch();
        inc_scene_trace_exact_path();
        inc_scene_trace_conservative_path();
        inc_scene_trace_hit(6, 3);
        assert_eq!(metrics_get_raw(METRIC_SCENE_TRACE_SUPPORT_PRUNED_BRANCH), 1);
        assert_eq!(metrics_get_raw(METRIC_SCENE_TRACE_CANDIDATE_BRANCH), 2);
        assert_eq!(metrics_get_raw(METRIC_SCENE_TRACE_EXACT_PATH), 1);
        assert_eq!(metrics_get_raw(METRIC_SCENE_TRACE_CONSERVATIVE_PATH), 1);
        assert_eq!(metrics_get_raw(METRIC_SCENE_TRACE_HIT_COUNT), 1);
        assert_eq!(metrics_get_raw(METRIC_SCENE_TRACE_HIT_STEPS_TOTAL), 6);
        assert_eq!(
            metrics_get_raw(METRIC_SCENE_TRACE_HIT_FIELD_SAMPLES_TOTAL),
            3
        );
        assert_eq!(metrics_get_raw(METRIC_SCENE_TRACE_STEPS_LE_8), 1);
        reset();
        assert_eq!(metrics_get_raw(METRIC_SCENE_TRACE_SUPPORT_PRUNED_BRANCH), 0);
        assert_eq!(metrics_get_raw(METRIC_SCENE_TRACE_CANDIDATE_BRANCH), 0);
        assert_eq!(metrics_get_raw(METRIC_SCENE_TRACE_EXACT_PATH), 0);
        assert_eq!(metrics_get_raw(METRIC_SCENE_TRACE_CONSERVATIVE_PATH), 0);
        assert_eq!(metrics_get_raw(METRIC_SCENE_TRACE_HIT_COUNT), 0);
        assert_eq!(metrics_get_raw(METRIC_SCENE_TRACE_HIT_STEPS_TOTAL), 0);
        assert_eq!(
            metrics_get_raw(METRIC_SCENE_TRACE_HIT_FIELD_SAMPLES_TOTAL),
            0
        );
        assert_eq!(metrics_get_raw(METRIC_SCENE_TRACE_STEPS_LE_8), 0);
    }

    #[test]
    fn blend_and_deformation_metrics_track_and_reset() {
        let _guard = test_lock().lock().expect("metrics test lock");
        if !is_enabled() {
            return;
        }
        reset();
        inc_scene_trace_blend_cost();
        inc_scene_trace_blend_cost();
        inc_scene_trace_deformation_cost();
        assert_eq!(metrics_get_raw(METRIC_SCENE_TRACE_BLEND_COST), 2);
        assert_eq!(metrics_get_raw(METRIC_SCENE_TRACE_DEFORMATION_COST), 1);
        reset();
        assert_eq!(metrics_get_raw(METRIC_SCENE_TRACE_BLEND_COST), 0);
        assert_eq!(metrics_get_raw(METRIC_SCENE_TRACE_DEFORMATION_COST), 0);
    }
}
