use crate::actor::{PoolHandle, PoolMessage, deliver_pool_message};
use crate::config::{
    pool_max_share_default, pool_min_share_default, sched_ready_cap, sched_shards, sched_tick_ms,
};
use crate::diagnostics;
use crate::metrics::{
    inc_pool_enqueue_after_retire, inc_pool_queue_full, inc_sched_dispatched,
    inc_sched_skipped_no_credit,
};
use crate::wr_rc_dec;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::Notify;
use tokio::time::{Duration, Instant};

static SHARDS: OnceLock<Vec<Arc<SchedulerShard>>> = OnceLock::new();
static POOL_COUNTER: AtomicU64 = AtomicU64::new(1);

pub struct SchedulerShard {
    ready: ReadyQueue,
    head: AtomicUsize,
    notify: Notify,
    has_work: AtomicBool,
    retired: Mutex<Vec<usize>>,
}

impl SchedulerShard {
    fn new(_id: usize) -> Self {
        Self {
            ready: ReadyQueue::new(sched_ready_cap()),
            head: AtomicUsize::new(0),
            notify: Notify::new(),
            has_work: AtomicBool::new(false),
            retired: Mutex::new(Vec::new()),
        }
    }
}

struct ReadySlot {
    seq: AtomicUsize,
    val: AtomicUsize,
}

struct ReadyQueue {
    mask: usize,
    slots: Box<[ReadySlot]>,
    head: AtomicUsize,
    tail: AtomicUsize,
}

impl ReadyQueue {
    fn new(cap: usize) -> Self {
        let cap = cap.next_power_of_two().max(2);
        let mut slots = Vec::with_capacity(cap);
        for i in 0..cap {
            slots.push(ReadySlot {
                seq: AtomicUsize::new(i),
                val: AtomicUsize::new(0),
            });
        }
        Self {
            mask: cap - 1,
            slots: slots.into_boxed_slice(),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    fn push(&self, value: usize) -> bool {
        let mut tail = self.tail.load(Ordering::Relaxed);
        loop {
            let slot = &self.slots[tail & self.mask];
            let seq = slot.seq.load(Ordering::Acquire);
            let diff = seq as isize - tail as isize;
            if diff == 0 {
                if self
                    .tail
                    .compare_exchange_weak(tail, tail + 1, Ordering::AcqRel, Ordering::Relaxed)
                    .is_ok()
                {
                    slot.val.store(value, Ordering::Relaxed);
                    slot.seq.store(tail + 1, Ordering::Release);
                    return true;
                }
            } else if diff < 0 {
                return false;
            }
            tail = self.tail.load(Ordering::Relaxed);
        }
    }

    fn pop(&self) -> Option<usize> {
        let mut head = self.head.load(Ordering::Relaxed);
        loop {
            let slot = &self.slots[head & self.mask];
            let seq = slot.seq.load(Ordering::Acquire);
            let diff = seq as isize - (head + 1) as isize;
            if diff == 0 {
                if self
                    .head
                    .compare_exchange_weak(head, head + 1, Ordering::AcqRel, Ordering::Relaxed)
                    .is_ok()
                {
                    let val = slot.val.load(Ordering::Relaxed);
                    slot.seq.store(head + self.mask + 1, Ordering::Release);
                    return Some(val);
                }
            } else if diff < 0 {
                return None;
            }
            head = self.head.load(Ordering::Relaxed);
        }
    }

    fn peek_has_data(&self) -> bool {
        let head = self.head.load(Ordering::Acquire);
        let slot = &self.slots[head & self.mask];
        let seq = slot.seq.load(Ordering::Acquire);
        let diff = seq as isize - (head + 1) as isize;
        diff >= 0
    }
}

pub fn init() -> &'static Vec<Arc<SchedulerShard>> {
    SHARDS.get_or_init(|| {
        let count = sched_shards();
        let mut shards = Vec::with_capacity(count);
        for id in 0..count {
            shards.push(Arc::new(SchedulerShard::new(id)));
        }
        let shard_handles = shards.clone();
        for shard in shards.iter() {
            let shard = shard.clone();
            crate::actor::runtime_spawn(async move {
                scheduler_loop(shard).await;
            });
        }
        shard_handles
    })
}

pub fn snapshot() -> String {
    let Some(shards) = SHARDS.get() else {
        return "scheduler: not initialized".to_string();
    };
    let mut out = String::new();
    out.push_str("scheduler:\n");
    for (idx, shard) in shards.iter().enumerate() {
        let mut pools = 0usize;
        let mut alive = 0usize;
        let mut queued = 0usize;
        let mut credits = 0i64;
        let mut pool_ptr = shard.head.load(Ordering::Acquire) as *const PoolHandle;
        while !pool_ptr.is_null() {
            unsafe {
                pools += 1;
                let pool = &*pool_ptr;
                if pool.alive.load(Ordering::Acquire) {
                    alive += 1;
                }
                queued = queued.saturating_add(pool.queue.len());
                credits = credits.saturating_add(pool.credits.load(Ordering::Relaxed));
                pool_ptr = pool.next_in_shard.load(Ordering::Acquire) as *const PoolHandle;
            }
        }
        let ready = shard.ready.peek_has_data();
        out.push_str(&format!(
            "  shard {idx}: pools={pools} alive={alive} queued={queued} credits={credits} ready={ready}\n"
        ));
    }
    out
}

pub fn register_pool(pool: *const PoolHandle) -> (u64, u32) {
    let shards = init();
    let pool_id = POOL_COUNTER.fetch_add(1, Ordering::Relaxed);
    let shard_id = (pool_id as usize) % shards.len();
    let shard = &shards[shard_id];
    let pool_ptr = pool as usize;
    loop {
        let head = shard.head.load(Ordering::Acquire);
        unsafe {
            (*pool).next_in_shard.store(head, Ordering::Release);
        }
        if shard
            .head
            .compare_exchange_weak(head, pool_ptr, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
        {
            break;
        }
    }
    shard.notify.notify_one();
    (pool_id, shard_id as u32)
}

pub fn retire_pool(pool: *const PoolHandle) {
    unsafe {
        if pool.is_null() {
            return;
        }
        diagnostics::log_event("pool_retire");
        let shard_id = (*pool).shard_id as usize;
        if let Some(shard) = SHARDS.get().and_then(|s| s.get(shard_id)) {
            let mut retired = shard.retired.lock().expect("retired lock");
            retired.push(pool as usize);
            shard.notify.notify_one();
        }
    }
}

pub fn enqueue(pool: *const PoolHandle, msg: PoolMessage) -> Result<(), PoolMessage> {
    unsafe {
        if pool.is_null() {
            return Err(msg);
        }
        (*pool).enqueue_inflight.fetch_add(1, Ordering::AcqRel);
        if !(*pool).alive.load(Ordering::Acquire) {
            inc_pool_enqueue_after_retire();
            diagnostics::log_event("enqueue_after_retire");
            (*pool).enqueue_inflight.fetch_sub(1, Ordering::AcqRel);
            return Err(msg);
        }
        if (*pool).drop_on_full {
            inc_pool_queue_full();
            (*pool).enqueue_inflight.fetch_sub(1, Ordering::AcqRel);
            return Err(msg);
        }
        let queue = &(*pool).queue;
        if let Err(msg) = queue.push(msg) {
            inc_pool_queue_full();
            (*pool).enqueue_inflight.fetch_sub(1, Ordering::AcqRel);
            return Err(msg);
        }
        (*pool).enqueue_inflight.fetch_sub(1, Ordering::AcqRel);
        let shard_id = (*pool).shard_id as usize;
        if let Some(shard) = SHARDS.get().and_then(|s| s.get(shard_id)) {
            if !(*pool).has_ready.swap(true, Ordering::AcqRel) {
                let _ = shard.ready.push(pool as usize);
            }
            if !shard.has_work.swap(true, Ordering::AcqRel) {
                shard.notify.notify_one();
            }
        }
        Ok(())
    }
}

async fn scheduler_loop(shard: Arc<SchedulerShard>) {
    let tick = Duration::from_millis(sched_tick_ms());
    let mut last_refill = Instant::now();
    let mut last_progress = Instant::now();
    let watchdog_ms = crate::config::sched_watchdog_ms();
    loop {
        if !shard.has_work.load(Ordering::Acquire) {
            let _ = tokio::time::timeout(tick, shard.notify.notified()).await;
        }
        if shard.ready.peek_has_data() {
            // Hot path: refill inline to avoid a full shard scan when idle.
            inline_refill_ready(&shard);
        } else {
            refill_shard(&shard, &mut last_refill);
        }
        let dispatched = dispatch_ready(&shard);
        if dispatched > 0 {
            last_progress = Instant::now();
        } else if watchdog_ms > 0 && last_progress.elapsed() >= Duration::from_millis(watchdog_ms) {
            eprintln!(
                "fatal: scheduler watchdog expired after {} ms without progress",
                watchdog_ms
            );
            std::process::abort();
        }
        reap_retired(&shard);
        if !shard.ready.peek_has_data() {
            shard.has_work.store(false, Ordering::Release);
        }
        steal_work(&shard);
    }
}

fn reap_retired(shard: &SchedulerShard) {
    let retired = {
        let mut guard = shard.retired.lock().expect("retired lock");
        if guard.is_empty() {
            return;
        }
        guard.drain(..).collect::<Vec<_>>()
    };
    for pool in retired {
        let pool = pool as *const PoolHandle;
        unsafe {
            if (*pool).enqueue_inflight.load(Ordering::Acquire) != 0 {
                let mut guard = shard.retired.lock().expect("retired lock");
                guard.push(pool as usize);
                continue;
            }
            diagnostics::log_event("retire_drain_start");
            while let Some(msg) = (*pool).queue.pop() {
                deliver_pool_message(msg);
            }
            diagnostics::log_event("retire_drain_done");
            for handle in (&(*pool).handles).iter() {
                wr_rc_dec(*handle);
            }
        }
        unlink_pool(shard, pool);
        unsafe { drop(Box::from_raw(pool as *mut PoolHandle)) };
    }
}

fn unlink_pool(shard: &SchedulerShard, target: *const PoolHandle) {
    loop {
        let head = shard.head.load(Ordering::Acquire) as *const PoolHandle;
        if head.is_null() {
            return;
        }
        if head == target {
            let next =
                unsafe { (*head).next_in_shard.load(Ordering::Acquire) as *const PoolHandle };
            if shard
                .head
                .compare_exchange(
                    head as usize,
                    next as usize,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                return;
            }
            continue;
        }
        let mut prev = head;
        let mut current =
            unsafe { (*prev).next_in_shard.load(Ordering::Acquire) as *const PoolHandle };
        while !current.is_null() {
            if current == target {
                let next = unsafe {
                    (*current).next_in_shard.load(Ordering::Acquire) as *const PoolHandle
                };
                unsafe {
                    (*prev)
                        .next_in_shard
                        .store(next as usize, Ordering::Release);
                }
                return;
            }
            prev = current;
            current = unsafe { (*prev).next_in_shard.load(Ordering::Acquire) as *const PoolHandle };
        }
        return;
    }
}

fn steal_work(shard: &SchedulerShard) {
    let Some(shards) = SHARDS.get() else { return };
    let self_ptr = shard as *const SchedulerShard as usize;
    for other in shards.iter() {
        let other_ptr = other.as_ref() as *const SchedulerShard as usize;
        if other_ptr == self_ptr {
            continue;
        }
        if other.ready.peek_has_data() {
            if let Some(pool_ptr) = other.ready.pop() {
                let _ = shard.ready.push(pool_ptr);
                shard.has_work.store(true, Ordering::Release);
                break;
            }
        }
    }
}

fn inline_refill_ready(shard: &SchedulerShard) {
    // Minimal refill to keep hot pools moving.
    let mut pool_ptr = shard.head.load(Ordering::Acquire) as *const PoolHandle;
    let mut budget = 32usize;
    while !pool_ptr.is_null() && budget > 0 {
        unsafe {
            let pool = &*pool_ptr;
            let min = if pool.min_share == 0 {
                pool_min_share_default()
            } else {
                pool.min_share
            } as i64;
            let max = if pool.max_share == 0 {
                pool_max_share_default()
            } else {
                pool.max_share
            } as i64;
            let weight = if pool.weight == 0 { 1 } else { pool.weight } as i64;
            let delta = min + weight;
            let mut current = pool.credits.load(Ordering::Relaxed);
            current = current.saturating_add(delta).min(max);
            pool.credits.store(current, Ordering::Relaxed);
            pool_ptr = pool.next_in_shard.load(Ordering::Acquire) as *const PoolHandle;
            budget -= 1;
        }
    }
}

fn refill_shard(shard: &SchedulerShard, last_refill: &mut Instant) {
    let now = Instant::now();
    if now.duration_since(*last_refill).as_millis() == 0 {
        return;
    }
    *last_refill = now;
    let mut pool_ptr = shard.head.load(Ordering::Acquire) as *const PoolHandle;
    while !pool_ptr.is_null() {
        unsafe {
            let pool = &*pool_ptr;
            let min = if pool.min_share == 0 {
                pool_min_share_default()
            } else {
                pool.min_share
            } as i64;
            let max = if pool.max_share == 0 {
                pool_max_share_default()
            } else {
                pool.max_share
            } as i64;
            let weight = if pool.weight == 0 { 1 } else { pool.weight } as i64;
            let delta = min + weight;
            let mut current = pool.credits.load(Ordering::Relaxed);
            current = current.saturating_add(delta).min(max);
            pool.credits.store(current, Ordering::Relaxed);
            pool_ptr = pool.next_in_shard.load(Ordering::Acquire) as *const PoolHandle;
        }
    }
}

fn dispatch_ready(shard: &SchedulerShard) -> i64 {
    let mut dispatched_total = 0i64;
    loop {
        let pool_ptr = shard.ready.pop();
        let Some(pool_ptr) = pool_ptr else {
            break;
        };
        let pool_ptr = pool_ptr as *const PoolHandle;
        if pool_ptr.is_null() {
            continue;
        }
        unsafe {
            let pool = &*pool_ptr;
            if !pool.alive.load(Ordering::Acquire) {
                continue;
            }
            let mut credits = pool.credits.load(Ordering::Relaxed);
            if credits <= 0 {
                inc_sched_skipped_no_credit();
                let _ = shard.ready.push(pool_ptr as usize);
                continue;
            }
            if let Some(msg) = pool.queue.pop() {
                let mut max_batch = pool.batch_limit.max(1);
                let depth = pool.queue.len() as i64;
                if depth > max_batch {
                    max_batch = (max_batch * 2).max(1);
                }
                let mut dispatched = 0i64;
                let mut first = Some(msg);
                while dispatched < max_batch && credits > 0 {
                    let msg = if let Some(msg) = first.take() {
                        msg
                    } else if pool.queue.has_more() {
                        match pool.queue.pop() {
                            Some(msg) => msg,
                            None => break,
                        }
                    } else {
                        break;
                    };
                    credits -= 1;
                    dispatched += 1;
                    crate::actor::deliver_pool_message(msg);
                    inc_sched_dispatched();
                }
                dispatched_total += dispatched;
                if dispatched > 0 {
                    pool.credits.store(credits, Ordering::Relaxed);
                }
                if pool.queue.has_more() {
                    let _ = shard.ready.push(pool_ptr as usize);
                } else {
                    pool.has_ready.store(false, Ordering::Release);
                }
            } else {
                pool.has_ready.store(false, Ordering::Release);
            }
        }
    }
    dispatched_total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor;
    use crate::list;
    use crate::value::Value;
    use crate::wr_rc_dec;
    use std::sync::Arc;
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[test]
    fn retire_pool_unlinks_head() {
        let actor_handle = actor::actor_spawn(1, Value::nil(), 1, 3, -1, -1, -1);
        let handles = list::list_new(0);
        list::list_push(handles, actor_handle);
        let pool = actor::pool_new(handles, 0, 0, 0, 0, -1);
        let pool_ptr = pool.as_ptr() as *const PoolHandle;
        let shard_id = unsafe { (*pool_ptr).shard_id as usize };
        unsafe {
            wr_rc_dec(pool);
        }
        let shards = init();
        let shard = shards[shard_id].clone();
        reap_retired(&shard);
        let head = shard.head.load(Ordering::Acquire) as *const PoolHandle;
        assert_ne!(head, pool_ptr);
        unsafe {
            wr_rc_dec(handles);
            wr_rc_dec(actor_handle);
        }
    }

    #[test]
    fn retire_pool_drains_and_delivers() {
        static COUNTER_PTR: OnceLock<Arc<AtomicUsize>> = OnceLock::new();

        extern "C" fn bump(_argc: usize, _argv: *const Value) -> Value {
            if let Some(counter) = COUNTER_PTR.get() {
                counter.fetch_add(1, Ordering::SeqCst);
            }
            Value::nil()
        }

        let counter = Arc::new(AtomicUsize::new(0));
        let _ = COUNTER_PTR.set(counter.clone());

        actor::register_method(99, 0, bump);
        let actor_handle = actor::actor_spawn(99, Value::nil(), 1, 3, -1, -1, -1);
        let handles = list::list_new(0);
        list::list_push(handles, actor_handle);
        let pool = actor::pool_new(handles, 0, 0, 0, 0, -1);
        actor::actor_fire(pool, 0, 0, std::ptr::null());

        let pool_ptr = pool.as_ptr() as *const PoolHandle;
        let shard_id = unsafe { (*pool_ptr).shard_id as usize };
        unsafe { wr_rc_dec(pool) };

        let shards = init();
        let shard = shards[shard_id].clone();
        reap_retired(&shard);

        for _ in 0..100 {
            if counter.load(Ordering::SeqCst) == 1 {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }

        assert_eq!(counter.load(Ordering::SeqCst), 1);
        unsafe {
            wr_rc_dec(handles);
            wr_rc_dec(actor_handle);
        }
    }

    #[test]
    fn retire_pool_drains_many_messages() {
        static COUNTER_PTR: OnceLock<Arc<AtomicUsize>> = OnceLock::new();

        extern "C" fn bump(_argc: usize, _argv: *const Value) -> Value {
            if let Some(counter) = COUNTER_PTR.get() {
                counter.fetch_add(1, Ordering::SeqCst);
            }
            Value::nil()
        }

        let counter = Arc::new(AtomicUsize::new(0));
        let _ = COUNTER_PTR.set(counter.clone());

        actor::register_method(100, 0, bump);
        let actor_handle = actor::actor_spawn(100, Value::nil(), 1, 3, -1, -1, -1);
        let handles = list::list_new(0);
        list::list_push(handles, actor_handle);
        let pool = actor::pool_new(handles, 0, 0, 0, 0, -1);

        let total = 128usize;
        for _ in 0..total {
            actor::actor_fire(pool, 0, 0, std::ptr::null());
        }

        let pool_ptr = pool.as_ptr() as *const PoolHandle;
        let shard_id = unsafe { (*pool_ptr).shard_id as usize };
        unsafe { wr_rc_dec(pool) };

        let shards = init();
        let shard = shards[shard_id].clone();
        reap_retired(&shard);

        for _ in 0..200 {
            if counter.load(Ordering::SeqCst) == total {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }

        assert_eq!(counter.load(Ordering::SeqCst), total);
        unsafe {
            wr_rc_dec(handles);
            wr_rc_dec(actor_handle);
        }
    }

    #[test]
    #[ignore]
    fn retire_pool_race_stress() {
        static COUNTER_PTR: OnceLock<Arc<AtomicUsize>> = OnceLock::new();

        extern "C" fn bump(_argc: usize, _argv: *const Value) -> Value {
            if let Some(counter) = COUNTER_PTR.get() {
                counter.fetch_add(1, Ordering::SeqCst);
            }
            Value::nil()
        }

        let counter = Arc::new(AtomicUsize::new(0));
        let _ = COUNTER_PTR.set(counter.clone());

        actor::register_method(101, 0, bump);
        let actor_handle = actor::actor_spawn(101, Value::nil(), 1, 3, -1, -1, -1);
        let handles = list::list_new(0);
        list::list_push(handles, actor_handle);
        let pool = actor::pool_new(handles, 0, 0, 0, 0, -1);

        let pool_ptr = pool.as_ptr() as *const PoolHandle;
        let shard_id = unsafe { (*pool_ptr).shard_id as usize };

        let sent = Arc::new(AtomicUsize::new(0));
        let mut threads = Vec::new();
        for _ in 0..4 {
            let sent = sent.clone();
            threads.push(std::thread::spawn(move || {
                for _ in 0..10_000 {
                    actor::actor_fire(pool, 0, 0, std::ptr::null());
                    sent.fetch_add(1, Ordering::SeqCst);
                    std::thread::yield_now();
                }
            }));
        }

        std::thread::sleep(Duration::from_millis(10));
        unsafe { wr_rc_dec(pool) };

        for t in threads {
            let _ = t.join();
        }

        let shards = init();
        let shard = shards[shard_id].clone();
        reap_retired(&shard);

        let delivered = counter.load(Ordering::SeqCst);
        let total_sent = sent.load(Ordering::SeqCst);
        assert!(delivered <= total_sent);

        unsafe {
            wr_rc_dec(handles);
            wr_rc_dec(actor_handle);
        }
    }
}
