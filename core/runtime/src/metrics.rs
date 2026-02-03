#[cfg(feature = "metrics")]
use serde::Serialize;
#[cfg(feature = "metrics")]
use std::fs;
#[cfg(feature = "metrics")]
use std::io;
#[cfg(feature = "metrics")]
use std::path::{Path, PathBuf};
#[cfg(feature = "metrics")]
use std::sync::OnceLock;
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
pub const METRIC_STORAGE_BACKUP_SUCCESS: u32 = 18;
pub const METRIC_STORAGE_BACKUP_FAILURE: u32 = 19;
pub const METRIC_STORAGE_BACKUP_LAST_DURATION_NS: u32 = 20;
pub const METRIC_STORAGE_BACKUP_LAST_SIZE: u32 = 21;
pub const METRIC_STORAGE_BACKUP_LAST_TS: u32 = 22;
pub const METRIC_STORAGE_BACKUP_RESTORE_FAILURE: u32 = 23;
pub const METRIC_PUBSUB_PUBLISH: u32 = 24;
pub const METRIC_PUBSUB_PUBLISH_FAILURE: u32 = 25;
pub const METRIC_SCHED_WAKEUPS: u32 = 26;
pub const METRIC_JOBS_WAKEUPS: u32 = 27;
pub const METRIC_ALLOC_LIST: u32 = 28;
pub const METRIC_ALLOC_MAP: u32 = 29;
pub const METRIC_ALLOC_STRING: u32 = 30;
pub const METRIC_ALLOC_BYTES: u32 = 31;
pub const METRIC_ALLOC_RESULT: u32 = 32;
pub const METRIC_ALLOC_PENDING: u32 = 33;
pub const METRIC_MAILBOX_ENQUEUE_OK: u32 = 34;
pub const METRIC_MAILBOX_ENQUEUE_FAIL: u32 = 35;
pub const METRIC_MAILBOX_DEQUEUE: u32 = 36;

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
    storage_backup_success: AtomicU64,
    storage_backup_failure: AtomicU64,
    storage_backup_last_duration_ns: AtomicU64,
    storage_backup_last_size: AtomicU64,
    storage_backup_last_ts: AtomicU64,
    storage_backup_restore_failure: AtomicU64,
    pubsub_publish: AtomicU64,
    pubsub_publish_failure: AtomicU64,
    sched_wakeups: AtomicU64,
    jobs_wakeups: AtomicU64,
    alloc_list: AtomicU64,
    alloc_map: AtomicU64,
    alloc_string: AtomicU64,
    alloc_bytes: AtomicU64,
    alloc_result: AtomicU64,
    alloc_pending: AtomicU64,
    mailbox_enqueue_ok: AtomicU64,
    mailbox_enqueue_fail: AtomicU64,
    mailbox_dequeue: AtomicU64,
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
    storage_backup_success: AtomicU64::new(0),
    storage_backup_failure: AtomicU64::new(0),
    storage_backup_last_duration_ns: AtomicU64::new(0),
    storage_backup_last_size: AtomicU64::new(0),
    storage_backup_last_ts: AtomicU64::new(0),
    storage_backup_restore_failure: AtomicU64::new(0),
    pubsub_publish: AtomicU64::new(0),
    pubsub_publish_failure: AtomicU64::new(0),
    sched_wakeups: AtomicU64::new(0),
    jobs_wakeups: AtomicU64::new(0),
    alloc_list: AtomicU64::new(0),
    alloc_map: AtomicU64::new(0),
    alloc_string: AtomicU64::new(0),
    alloc_bytes: AtomicU64::new(0),
    alloc_result: AtomicU64::new(0),
    alloc_pending: AtomicU64::new(0),
    mailbox_enqueue_ok: AtomicU64::new(0),
    mailbox_enqueue_fail: AtomicU64::new(0),
    mailbox_dequeue: AtomicU64::new(0),
};

#[cfg(feature = "metrics")]
#[derive(Serialize)]
struct MetricsSnapshot {
    messages_sent: u64,
    messages_dropped: u64,
    pending_resolved: u64,
    pending_dropped: u64,
    mailbox_high_water: u64,
    rc_inc: u64,
    rc_dec: u64,
    messages_dropped_paused: u64,
    sched_dispatched: u64,
    sched_skipped_no_credit: u64,
    pool_queue_full: u64,
    pool_enqueue_after_retire: u64,
    storage_batch_size: u64,
    storage_batch_latency_ns: u64,
    storage_commit_latency_ns: u64,
    storage_read_latency_ns: u64,
    storage_reads: u64,
    storage_batches: u64,
    storage_backup_success: u64,
    storage_backup_failure: u64,
    storage_backup_last_duration_ns: u64,
    storage_backup_last_size: u64,
    storage_backup_last_ts: u64,
    storage_backup_restore_failure: u64,
    pubsub_publish: u64,
    pubsub_publish_failure: u64,
    sched_wakeups: u64,
    jobs_wakeups: u64,
    alloc_list: u64,
    alloc_map: u64,
    alloc_string: u64,
    alloc_bytes: u64,
    alloc_result: u64,
    alloc_pending: u64,
    mailbox_enqueue_ok: u64,
    mailbox_enqueue_fail: u64,
    mailbox_dequeue: u64,
}

#[cfg(feature = "metrics")]
struct MetricsDumpGuard {
    path: Option<PathBuf>,
}

#[cfg(feature = "metrics")]
impl MetricsDumpGuard {
    fn new() -> Self {
        let path = std::env::var("WRELA_METRICS_PATH")
            .ok()
            .filter(|p| !p.is_empty())
            .map(PathBuf::from);
        Self { path }
    }
}

#[cfg(feature = "metrics")]
impl Drop for MetricsDumpGuard {
    fn drop(&mut self) {
        let Some(path) = &self.path else { return };
        let _ = dump_to_path(path);
    }
}

#[cfg(feature = "metrics")]
pub fn install_dump_hook() {
    static GUARD: OnceLock<MetricsDumpGuard> = OnceLock::new();
    GUARD.get_or_init(MetricsDumpGuard::new);
}

#[cfg(feature = "metrics")]
fn snapshot() -> MetricsSnapshot {
    MetricsSnapshot {
        messages_sent: METRICS.messages_sent.load(Ordering::Relaxed),
        messages_dropped: METRICS.messages_dropped.load(Ordering::Relaxed),
        pending_resolved: METRICS.pending_resolved.load(Ordering::Relaxed),
        pending_dropped: METRICS.pending_dropped.load(Ordering::Relaxed),
        mailbox_high_water: METRICS.mailbox_high_water.load(Ordering::Relaxed),
        rc_inc: METRICS.rc_inc.load(Ordering::Relaxed),
        rc_dec: METRICS.rc_dec.load(Ordering::Relaxed),
        messages_dropped_paused: METRICS.messages_dropped_paused.load(Ordering::Relaxed),
        sched_dispatched: METRICS.sched_dispatched.load(Ordering::Relaxed),
        sched_skipped_no_credit: METRICS.sched_skipped_no_credit.load(Ordering::Relaxed),
        pool_queue_full: METRICS.pool_queue_full.load(Ordering::Relaxed),
        pool_enqueue_after_retire: METRICS.pool_enqueue_after_retire.load(Ordering::Relaxed),
        storage_batch_size: METRICS.storage_batch_size.load(Ordering::Relaxed),
        storage_batch_latency_ns: METRICS.storage_batch_latency_ns.load(Ordering::Relaxed),
        storage_commit_latency_ns: METRICS.storage_commit_latency_ns.load(Ordering::Relaxed),
        storage_read_latency_ns: METRICS.storage_read_latency_ns.load(Ordering::Relaxed),
        storage_reads: METRICS.storage_reads.load(Ordering::Relaxed),
        storage_batches: METRICS.storage_batches.load(Ordering::Relaxed),
        storage_backup_success: METRICS.storage_backup_success.load(Ordering::Relaxed),
        storage_backup_failure: METRICS.storage_backup_failure.load(Ordering::Relaxed),
        storage_backup_last_duration_ns: METRICS
            .storage_backup_last_duration_ns
            .load(Ordering::Relaxed),
        storage_backup_last_size: METRICS.storage_backup_last_size.load(Ordering::Relaxed),
        storage_backup_last_ts: METRICS.storage_backup_last_ts.load(Ordering::Relaxed),
        storage_backup_restore_failure: METRICS
            .storage_backup_restore_failure
            .load(Ordering::Relaxed),
        pubsub_publish: METRICS.pubsub_publish.load(Ordering::Relaxed),
        pubsub_publish_failure: METRICS.pubsub_publish_failure.load(Ordering::Relaxed),
        sched_wakeups: METRICS.sched_wakeups.load(Ordering::Relaxed),
        jobs_wakeups: METRICS.jobs_wakeups.load(Ordering::Relaxed),
        alloc_list: METRICS.alloc_list.load(Ordering::Relaxed),
        alloc_map: METRICS.alloc_map.load(Ordering::Relaxed),
        alloc_string: METRICS.alloc_string.load(Ordering::Relaxed),
        alloc_bytes: METRICS.alloc_bytes.load(Ordering::Relaxed),
        alloc_result: METRICS.alloc_result.load(Ordering::Relaxed),
        alloc_pending: METRICS.alloc_pending.load(Ordering::Relaxed),
        mailbox_enqueue_ok: METRICS.mailbox_enqueue_ok.load(Ordering::Relaxed),
        mailbox_enqueue_fail: METRICS.mailbox_enqueue_fail.load(Ordering::Relaxed),
        mailbox_dequeue: METRICS.mailbox_dequeue.load(Ordering::Relaxed),
    }
}

#[cfg(feature = "metrics")]
fn dump_to_path(path: &Path) -> io::Result<()> {
    let snapshot = snapshot();
    let json = serde_json::to_vec(&snapshot)
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
    fs::write(path, json)
}

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
pub fn inc_pubsub_publish() {
    METRICS.pubsub_publish.fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn inc_pubsub_publish_failure() {
    METRICS
        .pubsub_publish_failure
        .fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn inc_sched_wakeup() {
    METRICS.sched_wakeups.fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn inc_jobs_wakeup() {
    METRICS.jobs_wakeups.fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn inc_alloc_list() {
    METRICS.alloc_list.fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn inc_alloc_map() {
    METRICS.alloc_map.fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn inc_alloc_string() {
    METRICS.alloc_string.fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn inc_alloc_bytes() {
    METRICS.alloc_bytes.fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn inc_alloc_result() {
    METRICS.alloc_result.fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn inc_alloc_pending() {
    METRICS.alloc_pending.fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn inc_mailbox_enqueue_ok() {
    METRICS.mailbox_enqueue_ok.fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn inc_mailbox_enqueue_fail() {
    METRICS.mailbox_enqueue_fail.fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn inc_mailbox_dequeue() {
    METRICS.mailbox_dequeue.fetch_add(1, Ordering::Relaxed);
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
pub fn inc_storage_backup_success() {
    METRICS
        .storage_backup_success
        .fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn inc_storage_backup_failure() {
    METRICS
        .storage_backup_failure
        .fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn record_storage_backup_duration(duration: std::time::Duration) {
    METRICS
        .storage_backup_last_duration_ns
        .store(duration.as_nanos() as u64, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn metrics_get_raw(id: u32) -> u64 {
    match id {
        METRIC_MESSAGES_SENT => METRICS.messages_sent.load(Ordering::Relaxed),
        METRIC_MESSAGES_DROPPED => METRICS.messages_dropped.load(Ordering::Relaxed),
        METRIC_PENDING_RESOLVED => METRICS.pending_resolved.load(Ordering::Relaxed),
        METRIC_PENDING_DROPPED => METRICS.pending_dropped.load(Ordering::Relaxed),
        METRIC_MAILBOX_HIGH_WATER => METRICS.mailbox_high_water.load(Ordering::Relaxed),
        METRIC_RC_INC => METRICS.rc_inc.load(Ordering::Relaxed),
        METRIC_RC_DEC => METRICS.rc_dec.load(Ordering::Relaxed),
        METRIC_MESSAGES_DROPPED_PAUSED => METRICS.messages_dropped_paused.load(Ordering::Relaxed),
        METRIC_SCHED_DISPATCHED => METRICS.sched_dispatched.load(Ordering::Relaxed),
        METRIC_SCHED_SKIPPED_NO_CREDIT => METRICS.sched_skipped_no_credit.load(Ordering::Relaxed),
        METRIC_POOL_QUEUE_FULL => METRICS.pool_queue_full.load(Ordering::Relaxed),
        METRIC_POOL_ENQUEUE_AFTER_RETIRE => {
            METRICS.pool_enqueue_after_retire.load(Ordering::Relaxed)
        }
        METRIC_STORAGE_BATCH_SIZE => METRICS.storage_batch_size.load(Ordering::Relaxed),
        METRIC_STORAGE_BATCH_LATENCY_NS => METRICS.storage_batch_latency_ns.load(Ordering::Relaxed),
        METRIC_STORAGE_COMMIT_LATENCY_NS => {
            METRICS.storage_commit_latency_ns.load(Ordering::Relaxed)
        }
        METRIC_STORAGE_READ_LATENCY_NS => METRICS.storage_read_latency_ns.load(Ordering::Relaxed),
        METRIC_STORAGE_READS => METRICS.storage_reads.load(Ordering::Relaxed),
        METRIC_STORAGE_BATCHES => METRICS.storage_batches.load(Ordering::Relaxed),
        METRIC_STORAGE_BACKUP_SUCCESS => METRICS.storage_backup_success.load(Ordering::Relaxed),
        METRIC_STORAGE_BACKUP_FAILURE => METRICS.storage_backup_failure.load(Ordering::Relaxed),
        METRIC_STORAGE_BACKUP_LAST_DURATION_NS => METRICS
            .storage_backup_last_duration_ns
            .load(Ordering::Relaxed),
        METRIC_STORAGE_BACKUP_LAST_SIZE => METRICS.storage_backup_last_size.load(Ordering::Relaxed),
        METRIC_STORAGE_BACKUP_LAST_TS => METRICS.storage_backup_last_ts.load(Ordering::Relaxed),
        METRIC_STORAGE_BACKUP_RESTORE_FAILURE => METRICS
            .storage_backup_restore_failure
            .load(Ordering::Relaxed),
        METRIC_PUBSUB_PUBLISH => METRICS.pubsub_publish.load(Ordering::Relaxed),
        METRIC_PUBSUB_PUBLISH_FAILURE => {
            METRICS.pubsub_publish_failure.load(Ordering::Relaxed)
        }
        METRIC_SCHED_WAKEUPS => METRICS.sched_wakeups.load(Ordering::Relaxed),
        METRIC_JOBS_WAKEUPS => METRICS.jobs_wakeups.load(Ordering::Relaxed),
        METRIC_ALLOC_LIST => METRICS.alloc_list.load(Ordering::Relaxed),
        METRIC_ALLOC_MAP => METRICS.alloc_map.load(Ordering::Relaxed),
        METRIC_ALLOC_STRING => METRICS.alloc_string.load(Ordering::Relaxed),
        METRIC_ALLOC_BYTES => METRICS.alloc_bytes.load(Ordering::Relaxed),
        METRIC_ALLOC_RESULT => METRICS.alloc_result.load(Ordering::Relaxed),
        METRIC_ALLOC_PENDING => METRICS.alloc_pending.load(Ordering::Relaxed),
        METRIC_MAILBOX_ENQUEUE_OK => METRICS.mailbox_enqueue_ok.load(Ordering::Relaxed),
        METRIC_MAILBOX_ENQUEUE_FAIL => METRICS.mailbox_enqueue_fail.load(Ordering::Relaxed),
        METRIC_MAILBOX_DEQUEUE => METRICS.mailbox_dequeue.load(Ordering::Relaxed),
        _ => 0,
    }
}

#[cfg(not(feature = "metrics"))]
pub fn metrics_get_raw(_id: u32) -> u64 {
    0
}

#[cfg(feature = "metrics")]
pub fn record_storage_backup_size(size: usize) {
    METRICS
        .storage_backup_last_size
        .store(size as u64, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn record_storage_backup_ts(ts_secs: u64) {
    METRICS
        .storage_backup_last_ts
        .store(ts_secs, Ordering::Relaxed);
}

#[cfg(feature = "metrics")]
pub fn inc_storage_backup_restore_failure() {
    METRICS
        .storage_backup_restore_failure
        .fetch_add(1, Ordering::Relaxed);
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
        METRIC_MESSAGES_DROPPED_PAUSED => METRICS.messages_dropped_paused.load(Ordering::Relaxed),
        METRIC_SCHED_DISPATCHED => METRICS.sched_dispatched.load(Ordering::Relaxed),
        METRIC_SCHED_SKIPPED_NO_CREDIT => METRICS.sched_skipped_no_credit.load(Ordering::Relaxed),
        METRIC_POOL_QUEUE_FULL => METRICS.pool_queue_full.load(Ordering::Relaxed),
        METRIC_POOL_ENQUEUE_AFTER_RETIRE => {
            METRICS.pool_enqueue_after_retire.load(Ordering::Relaxed)
        }
        METRIC_STORAGE_BATCH_SIZE => METRICS.storage_batch_size.load(Ordering::Relaxed),
        METRIC_STORAGE_BATCH_LATENCY_NS => METRICS.storage_batch_latency_ns.load(Ordering::Relaxed),
        METRIC_STORAGE_COMMIT_LATENCY_NS => {
            METRICS.storage_commit_latency_ns.load(Ordering::Relaxed)
        }
        METRIC_STORAGE_READ_LATENCY_NS => METRICS.storage_read_latency_ns.load(Ordering::Relaxed),
        METRIC_STORAGE_READS => METRICS.storage_reads.load(Ordering::Relaxed),
        METRIC_STORAGE_BATCHES => METRICS.storage_batches.load(Ordering::Relaxed),
        METRIC_STORAGE_BACKUP_SUCCESS => METRICS.storage_backup_success.load(Ordering::Relaxed),
        METRIC_STORAGE_BACKUP_FAILURE => METRICS.storage_backup_failure.load(Ordering::Relaxed),
        METRIC_STORAGE_BACKUP_LAST_DURATION_NS => METRICS
            .storage_backup_last_duration_ns
            .load(Ordering::Relaxed),
        METRIC_STORAGE_BACKUP_LAST_SIZE => METRICS.storage_backup_last_size.load(Ordering::Relaxed),
        METRIC_STORAGE_BACKUP_LAST_TS => METRICS.storage_backup_last_ts.load(Ordering::Relaxed),
        METRIC_STORAGE_BACKUP_RESTORE_FAILURE => METRICS
            .storage_backup_restore_failure
            .load(Ordering::Relaxed),
        METRIC_PUBSUB_PUBLISH => METRICS.pubsub_publish.load(Ordering::Relaxed),
        METRIC_PUBSUB_PUBLISH_FAILURE => {
            METRICS.pubsub_publish_failure.load(Ordering::Relaxed)
        }
        METRIC_SCHED_WAKEUPS => METRICS.sched_wakeups.load(Ordering::Relaxed),
        METRIC_JOBS_WAKEUPS => METRICS.jobs_wakeups.load(Ordering::Relaxed),
        METRIC_ALLOC_LIST => METRICS.alloc_list.load(Ordering::Relaxed),
        METRIC_ALLOC_MAP => METRICS.alloc_map.load(Ordering::Relaxed),
        METRIC_ALLOC_STRING => METRICS.alloc_string.load(Ordering::Relaxed),
        METRIC_ALLOC_BYTES => METRICS.alloc_bytes.load(Ordering::Relaxed),
        METRIC_ALLOC_RESULT => METRICS.alloc_result.load(Ordering::Relaxed),
        METRIC_ALLOC_PENDING => METRICS.alloc_pending.load(Ordering::Relaxed),
        METRIC_MAILBOX_ENQUEUE_OK => METRICS.mailbox_enqueue_ok.load(Ordering::Relaxed),
        METRIC_MAILBOX_ENQUEUE_FAIL => METRICS.mailbox_enqueue_fail.load(Ordering::Relaxed),
        METRIC_MAILBOX_DEQUEUE => METRICS.mailbox_dequeue.load(Ordering::Relaxed),
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
    METRICS.sched_skipped_no_credit.store(0, Ordering::Relaxed);
    METRICS.pool_queue_full.store(0, Ordering::Relaxed);
    METRICS
        .pool_enqueue_after_retire
        .store(0, Ordering::Relaxed);
    METRICS.storage_batch_size.store(0, Ordering::Relaxed);
    METRICS.storage_batch_latency_ns.store(0, Ordering::Relaxed);
    METRICS
        .storage_commit_latency_ns
        .store(0, Ordering::Relaxed);
    METRICS.storage_read_latency_ns.store(0, Ordering::Relaxed);
    METRICS.storage_reads.store(0, Ordering::Relaxed);
    METRICS.storage_batches.store(0, Ordering::Relaxed);
    METRICS.storage_backup_success.store(0, Ordering::Relaxed);
    METRICS.storage_backup_failure.store(0, Ordering::Relaxed);
    METRICS
        .storage_backup_last_duration_ns
        .store(0, Ordering::Relaxed);
    METRICS.storage_backup_last_size.store(0, Ordering::Relaxed);
    METRICS.storage_backup_last_ts.store(0, Ordering::Relaxed);
    METRICS
        .storage_backup_restore_failure
        .store(0, Ordering::Relaxed);
    METRICS.alloc_list.store(0, Ordering::Relaxed);
    METRICS.alloc_map.store(0, Ordering::Relaxed);
    METRICS.alloc_string.store(0, Ordering::Relaxed);
    METRICS.alloc_bytes.store(0, Ordering::Relaxed);
    METRICS.alloc_result.store(0, Ordering::Relaxed);
    METRICS.alloc_pending.store(0, Ordering::Relaxed);
    METRICS.mailbox_enqueue_ok.store(0, Ordering::Relaxed);
    METRICS.mailbox_enqueue_fail.store(0, Ordering::Relaxed);
    METRICS.mailbox_dequeue.store(0, Ordering::Relaxed);
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
pub fn inc_storage_backup_success() {}
#[cfg(not(feature = "metrics"))]
pub fn inc_storage_backup_failure() {}
#[cfg(not(feature = "metrics"))]
pub fn record_storage_backup_duration(_duration: std::time::Duration) {}
#[cfg(not(feature = "metrics"))]
pub fn record_storage_backup_size(_size: usize) {}
#[cfg(not(feature = "metrics"))]
pub fn record_storage_backup_ts(_ts_secs: u64) {}
#[cfg(not(feature = "metrics"))]
pub fn inc_storage_backup_restore_failure() {}
#[cfg(not(feature = "metrics"))]
pub fn inc_alloc_list() {}
#[cfg(not(feature = "metrics"))]
pub fn inc_alloc_map() {}
#[cfg(not(feature = "metrics"))]
pub fn inc_alloc_string() {}
#[cfg(not(feature = "metrics"))]
pub fn inc_alloc_bytes() {}
#[cfg(not(feature = "metrics"))]
pub fn inc_alloc_result() {}
#[cfg(not(feature = "metrics"))]
pub fn inc_alloc_pending() {}
#[cfg(not(feature = "metrics"))]
pub fn inc_mailbox_enqueue_ok() {}
#[cfg(not(feature = "metrics"))]
pub fn inc_mailbox_enqueue_fail() {}
#[cfg(not(feature = "metrics"))]
pub fn inc_mailbox_dequeue() {}
#[cfg(not(feature = "metrics"))]
pub fn reset() {}
