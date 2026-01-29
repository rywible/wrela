#[cfg(feature = "metrics")]
use std::sync::atomic::{AtomicU64, Ordering};

pub const METRIC_MESSAGES_SENT: u32 = 0;
pub const METRIC_MESSAGES_DROPPED: u32 = 1;
pub const METRIC_PENDING_RESOLVED: u32 = 2;
pub const METRIC_PENDING_DROPPED: u32 = 3;
pub const METRIC_MAILBOX_HIGH_WATER: u32 = 4;
pub const METRIC_RC_INC: u32 = 5;
pub const METRIC_RC_DEC: u32 = 6;
pub const METRIC_MESSAGES_DROPPED_PAUSED: u32 = 7;
pub const METRIC_SCHED_DISPATCHED: u32 = 8;
pub const METRIC_SCHED_SKIPPED_NO_CREDIT: u32 = 9;
pub const METRIC_POOL_QUEUE_FULL: u32 = 10;
pub const METRIC_POOL_ENQUEUE_AFTER_RETIRE: u32 = 11;
pub const METRIC_STORAGE_BATCH_SIZE: u32 = 12;
pub const METRIC_STORAGE_BATCH_LATENCY_NS: u32 = 13;
pub const METRIC_STORAGE_COMMIT_LATENCY_NS: u32 = 14;
pub const METRIC_STORAGE_READ_LATENCY_NS: u32 = 15;
pub const METRIC_STORAGE_READS: u32 = 16;
pub const METRIC_STORAGE_BATCHES: u32 = 17;

#[cfg(feature = "metrics")]
struct Metrics {
    messages_sent: AtomicU64,
    messages_dropped: AtomicU64,
    pending_resolved: AtomicU64,
    pending_dropped: AtomicU64,
    mailbox_high_water: AtomicU64,
    rc_inc: AtomicU64,
    rc_dec: AtomicU64,
    messages_dropped_paused: AtomicU64,
    sched_dispatched: AtomicU64,
    sched_skipped_no_credit: AtomicU64,
    pool_queue_full: AtomicU64,
    pool_enqueue_after_retire: AtomicU64,
    storage_batch_size: AtomicU64,
    storage_batch_latency_ns: AtomicU64,
    storage_commit_latency_ns: AtomicU64,
    storage_read_latency_ns: AtomicU64,
    storage_reads: AtomicU64,
    storage_batches: AtomicU64,
}

#[cfg(feature = "metrics")]
static METRICS: Metrics = Metrics {
    messages_sent: AtomicU64::new(0),
    messages_dropped: AtomicU64::new(0),
    pending_resolved: AtomicU64::new(0),
    pending_dropped: AtomicU64::new(0),
    mailbox_high_water: AtomicU64::new(0),
    rc_inc: AtomicU64::new(0),
    rc_dec: AtomicU64::new(0),
    messages_dropped_paused: AtomicU64::new(0),
    sched_dispatched: AtomicU64::new(0),
    sched_skipped_no_credit: AtomicU64::new(0),
    pool_queue_full: AtomicU64::new(0),
    pool_enqueue_after_retire: AtomicU64::new(0),
    storage_batch_size: AtomicU64::new(0),
    storage_batch_latency_ns: AtomicU64::new(0),
    storage_commit_latency_ns: AtomicU64::new(0),
    storage_read_latency_ns: AtomicU64::new(0),
    storage_reads: AtomicU64::new(0),
    storage_batches: AtomicU64::new(0),
};

#[cfg(feature = "metrics")]
pub fn inc_messages_sent() {
    METRICS.messages_sent.fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn inc_messages_dropped() {
    METRICS.messages_dropped.fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn inc_pending_resolved() {
    METRICS.pending_resolved.fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn inc_pending_dropped() {
    METRICS.pending_dropped.fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn inc_rc_inc() {
    METRICS.rc_inc.fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn inc_rc_dec() {
    METRICS.rc_dec.fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn inc_messages_dropped_paused() {
    METRICS
        .messages_dropped_paused
        .fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn inc_sched_dispatched() {
    METRICS.sched_dispatched.fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn inc_sched_skipped_no_credit() {
    METRICS
        .sched_skipped_no_credit
        .fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn inc_pool_queue_full() {
    METRICS.pool_queue_full.fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn inc_pool_enqueue_after_retire() {
    METRICS
        .pool_enqueue_after_retire
        .fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn record_storage_batch_size(size: usize) {
    METRICS
        .storage_batch_size
        .store(size as u64, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn record_storage_batch_latency(duration: std::time::Duration) {
    METRICS
        .storage_batch_latency_ns
        .store(duration.as_nanos() as u64, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn record_storage_commit_latency(duration: std::time::Duration) {
    METRICS
        .storage_commit_latency_ns
        .store(duration.as_nanos() as u64, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn record_storage_read_latency(duration: std::time::Duration) {
    METRICS
        .storage_read_latency_ns
        .store(duration.as_nanos() as u64, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn inc_storage_read() {
    METRICS.storage_reads.fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn inc_storage_batch_open() {
    METRICS.storage_batches.fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn update_mailbox_high_water(len: usize) {
    let mut current = METRICS.mailbox_high_water.load(Ordering::Relaxed);
    let target = len as u64;
    while target > current {
        match METRICS.mailbox_high_water.compare_exchange_weak(
            current,
            target,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return,
            Err(next) => current = next,
        }
    }
}

#[cfg(feature = "metrics")]
pub fn get(id: u32) -> u64 {
    match id {
        METRIC_MESSAGES_SENT => METRICS.messages_sent.load(Ordering::Relaxed),
        METRIC_MESSAGES_DROPPED => METRICS.messages_dropped.load(Ordering::Relaxed),
        METRIC_PENDING_RESOLVED => METRICS.pending_resolved.load(Ordering::Relaxed),
        METRIC_PENDING_DROPPED => METRICS.pending_dropped.load(Ordering::Relaxed),
        METRIC_MAILBOX_HIGH_WATER => METRICS.mailbox_high_water.load(Ordering::Relaxed),
        METRIC_RC_INC => METRICS.rc_inc.load(Ordering::Relaxed),
        METRIC_RC_DEC => METRICS.rc_dec.load(Ordering::Relaxed),
        METRIC_MESSAGES_DROPPED_PAUSED => {
            METRICS.messages_dropped_paused.load(Ordering::Relaxed)
        }
        METRIC_SCHED_DISPATCHED => METRICS.sched_dispatched.load(Ordering::Relaxed),
        METRIC_SCHED_SKIPPED_NO_CREDIT => {
            METRICS.sched_skipped_no_credit.load(Ordering::Relaxed)
        }
        METRIC_POOL_QUEUE_FULL => METRICS.pool_queue_full.load(Ordering::Relaxed),
        METRIC_POOL_ENQUEUE_AFTER_RETIRE => {
            METRICS.pool_enqueue_after_retire.load(Ordering::Relaxed)
        }
        METRIC_STORAGE_BATCH_SIZE => METRICS.storage_batch_size.load(Ordering::Relaxed),
        METRIC_STORAGE_BATCH_LATENCY_NS => {
            METRICS.storage_batch_latency_ns.load(Ordering::Relaxed)
        }
        METRIC_STORAGE_COMMIT_LATENCY_NS => {
            METRICS.storage_commit_latency_ns.load(Ordering::Relaxed)
        }
        METRIC_STORAGE_READ_LATENCY_NS => {
            METRICS.storage_read_latency_ns.load(Ordering::Relaxed)
        }
        METRIC_STORAGE_READS => METRICS.storage_reads.load(Ordering::Relaxed),
        METRIC_STORAGE_BATCHES => METRICS.storage_batches.load(Ordering::Relaxed),
        _ => 0,
    }
}

#[cfg(feature = "metrics")]
pub fn reset() {
    METRICS.messages_sent.store(0, Ordering::Relaxed);
    METRICS.messages_dropped.store(0, Ordering::Relaxed);
    METRICS.pending_resolved.store(0, Ordering::Relaxed);
    METRICS.pending_dropped.store(0, Ordering::Relaxed);
    METRICS.mailbox_high_water.store(0, Ordering::Relaxed);
    METRICS.rc_inc.store(0, Ordering::Relaxed);
    METRICS.rc_dec.store(0, Ordering::Relaxed);
    METRICS.messages_dropped_paused.store(0, Ordering::Relaxed);
    METRICS.sched_dispatched.store(0, Ordering::Relaxed);
    METRICS
        .sched_skipped_no_credit
        .store(0, Ordering::Relaxed);
    METRICS.pool_queue_full.store(0, Ordering::Relaxed);
    METRICS
        .pool_enqueue_after_retire
        .store(0, Ordering::Relaxed);
    METRICS.storage_batch_size.store(0, Ordering::Relaxed);
    METRICS
        .storage_batch_latency_ns
        .store(0, Ordering::Relaxed);
    METRICS
        .storage_commit_latency_ns
        .store(0, Ordering::Relaxed);
    METRICS
        .storage_read_latency_ns
        .store(0, Ordering::Relaxed);
    METRICS.storage_reads.store(0, Ordering::Relaxed);
    METRICS.storage_batches.store(0, Ordering::Relaxed);
}

#[cfg(not(feature = "metrics"))]
pub fn inc_messages_sent() {}
#[cfg(not(feature = "metrics"))]
pub fn inc_messages_dropped() {}
#[cfg(not(feature = "metrics"))]
pub fn inc_pending_resolved() {}
#[cfg(not(feature = "metrics"))]
pub fn inc_pending_dropped() {}
#[cfg(not(feature = "metrics"))]
pub fn inc_rc_inc() {}
#[cfg(not(feature = "metrics"))]
pub fn inc_rc_dec() {}
#[cfg(not(feature = "metrics"))]
pub fn inc_messages_dropped_paused() {}
#[cfg(not(feature = "metrics"))]
pub fn inc_sched_dispatched() {}
#[cfg(not(feature = "metrics"))]
pub fn inc_sched_skipped_no_credit() {}
#[cfg(not(feature = "metrics"))]
pub fn inc_pool_queue_full() {}
#[cfg(not(feature = "metrics"))]
pub fn inc_pool_enqueue_after_retire() {}
#[cfg(not(feature = "metrics"))]
pub fn update_mailbox_high_water(_len: usize) {}
#[cfg(not(feature = "metrics"))]
pub fn get(_id: u32) -> u64 {
    0
}
#[cfg(not(feature = "metrics"))]
pub fn record_storage_batch_size(_size: usize) {}
#[cfg(not(feature = "metrics"))]
pub fn record_storage_batch_latency(_duration: std::time::Duration) {}
#[cfg(not(feature = "metrics"))]
pub fn record_storage_commit_latency(_duration: std::time::Duration) {}
#[cfg(not(feature = "metrics"))]
pub fn record_storage_read_latency(_duration: std::time::Duration) {}
#[cfg(not(feature = "metrics"))]
pub fn inc_storage_read() {}
#[cfg(not(feature = "metrics"))]
pub fn inc_storage_batch_open() {}
#[cfg(not(feature = "metrics"))]
pub fn reset() {}
