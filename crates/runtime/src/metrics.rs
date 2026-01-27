use std::sync::atomic::{AtomicU64, Ordering};

pub const METRIC_MESSAGES_SENT: u32 = 0;
pub const METRIC_MESSAGES_DROPPED: u32 = 1;
pub const METRIC_PENDING_RESOLVED: u32 = 2;
pub const METRIC_PENDING_DROPPED: u32 = 3;
pub const METRIC_MAILBOX_HIGH_WATER: u32 = 4;
pub const METRIC_RC_INC: u32 = 5;
pub const METRIC_RC_DEC: u32 = 6;
pub const METRIC_MESSAGES_DROPPED_PAUSED: u32 = 7;

struct Metrics {
    messages_sent: AtomicU64,
    messages_dropped: AtomicU64,
    pending_resolved: AtomicU64,
    pending_dropped: AtomicU64,
    mailbox_high_water: AtomicU64,
    rc_inc: AtomicU64,
    rc_dec: AtomicU64,
    messages_dropped_paused: AtomicU64,
}

static METRICS: Metrics = Metrics {
    messages_sent: AtomicU64::new(0),
    messages_dropped: AtomicU64::new(0),
    pending_resolved: AtomicU64::new(0),
    pending_dropped: AtomicU64::new(0),
    mailbox_high_water: AtomicU64::new(0),
    rc_inc: AtomicU64::new(0),
    rc_dec: AtomicU64::new(0),
    messages_dropped_paused: AtomicU64::new(0),
};

pub fn inc_messages_sent() {
    METRICS.messages_sent.fetch_add(1, Ordering::Relaxed);
}

pub fn inc_messages_dropped() {
    METRICS.messages_dropped.fetch_add(1, Ordering::Relaxed);
}

pub fn inc_pending_resolved() {
    METRICS.pending_resolved.fetch_add(1, Ordering::Relaxed);
}

pub fn inc_pending_dropped() {
    METRICS.pending_dropped.fetch_add(1, Ordering::Relaxed);
}

pub fn inc_rc_inc() {
    METRICS.rc_inc.fetch_add(1, Ordering::Relaxed);
}

pub fn inc_rc_dec() {
    METRICS.rc_dec.fetch_add(1, Ordering::Relaxed);
}

pub fn inc_messages_dropped_paused() {
    METRICS
        .messages_dropped_paused
        .fetch_add(1, Ordering::Relaxed);
}

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
        _ => 0,
    }
}

pub fn reset() {
    METRICS.messages_sent.store(0, Ordering::Relaxed);
    METRICS.messages_dropped.store(0, Ordering::Relaxed);
    METRICS.pending_resolved.store(0, Ordering::Relaxed);
    METRICS.pending_dropped.store(0, Ordering::Relaxed);
    METRICS.mailbox_high_water.store(0, Ordering::Relaxed);
    METRICS.rc_inc.store(0, Ordering::Relaxed);
    METRICS.rc_dec.store(0, Ordering::Relaxed);
    METRICS.messages_dropped_paused.store(0, Ordering::Relaxed);
}
