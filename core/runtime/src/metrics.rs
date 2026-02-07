use std::sync::atomic::{AtomicU64, Ordering};

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

const METRIC_COUNT: usize = 64;
static METRICS: [AtomicU64; METRIC_COUNT] = [const { AtomicU64::new(0) }; METRIC_COUNT];
static MAILBOX_HIGH_WATER: AtomicU64 = AtomicU64::new(0);

fn bump(id: u32) {
    if let Some(metric) = METRICS.get(id as usize) {
        metric.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn get(id: u32) -> u64 {
    METRICS
        .get(id as usize)
        .map(|metric| metric.load(Ordering::Relaxed))
        .unwrap_or(0)
}

pub fn metrics_get_raw(id: u32) -> u64 {
    get(id)
}

pub fn reset() {
    for metric in METRICS.iter() {
        metric.store(0, Ordering::Relaxed);
    }
    MAILBOX_HIGH_WATER.store(0, Ordering::Relaxed);
}

pub fn install_dump_hook() {}

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
pub fn inc_pending_dropped() {
    bump(METRIC_PENDING_DROPPED)
}
pub fn inc_pending_resolved() {
    bump(METRIC_PENDING_RESOLVED)
}
pub fn inc_mailbox_enqueue_ok() {
    bump(METRIC_MAILBOX_ENQUEUE_OK)
}
pub fn inc_mailbox_enqueue_fail() {
    bump(METRIC_MAILBOX_ENQUEUE_FAIL)
}
pub fn inc_mailbox_dequeue() {
    bump(METRIC_MAILBOX_DEQUEUE)
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

pub fn update_mailbox_high_water(len: usize) {
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
