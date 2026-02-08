pub(crate) mod actor {
    use crate::arena;
    use crate::config;
    #[cfg(feature = "metrics")]
    use crate::metrics::inc_alloc_pending;
    use crate::metrics::{
        inc_mailbox_dequeue, inc_mailbox_enqueue_fail, inc_mailbox_enqueue_ok,
        inc_messages_dropped, inc_messages_dropped_paused, inc_messages_sent, inc_pending_dropped,
        inc_pending_resolved, update_mailbox_high_water,
    };
    use crate::object::ObjHeader;
    use crate::reactor::task::TaskSignal;
    use crate::result;
    use crate::scheduler;
    use crate::value::{TypeId, Value, header};
    use crate::{wr_rc_dec, wr_rc_inc};
    use std::cell::UnsafeCell;
    use std::collections::HashMap;
    use std::mem::MaybeUninit;
    use std::sync::atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::sync::mpsc::{TryRecvError, TrySendError};
    use std::sync::{Arc, Mutex, OnceLock};
    use std::time::Duration;

    pub type MethodFn = extern "C" fn(argc: usize, argv: *const Value) -> Value;

    #[repr(C)]
    pub struct ActorHandle {
        header: ObjHeader,
        class_id: u32,
        pool_size: i64,
        pub objective: u8,
        mailbox: Arc<Mailbox>,
        instance: Value,
    }

    #[repr(C)]
    pub struct PoolHandle {
        header: ObjHeader,
        pub pool_id: u64,
        pub shard_id: u32,
        pub objective: u8,
        pool_size: usize,
        rr: AtomicUsize,
        pub(crate) handles: Vec<Value>,
        pub queue: PoolQueue,
        pub credits: AtomicI64,
        pub min_share: u32,
        pub max_share: u32,
        pub weight: u32,
        pub has_ready: AtomicBool,
        pub alive: AtomicBool,
        pub enqueue_inflight: AtomicUsize,
        pub next_in_shard: AtomicUsize,
        pub batch_limit: i64,
        pub drop_on_full: bool,
        pub shard_hint: AtomicUsize,
    }

    #[repr(C)]
    pub struct PendingObj {
        header: ObjHeader,
        state: Arc<PendingState>,
    }

    struct Message {
        method_id: u32,
        args: Vec<Value>,
        pending: Option<Arc<PendingState>>,
    }

    pub struct PoolMessage {
        mailbox: Arc<Mailbox>,
        msg: Message,
    }

    pub struct PoolQueue {
        cap: usize,
        mask: usize,
        head: AtomicUsize,
        tail: AtomicUsize,
        slots: Box<[PoolSlot]>,
    }

    struct PoolSlot {
        seq: AtomicUsize,
        msg: UnsafeCell<MaybeUninit<PoolMessage>>,
    }

    unsafe impl Sync for PoolSlot {}

    struct Mailbox {
        sender: Mutex<Option<mpsc::SyncSender<Message>>>,
        len: AtomicUsize,
        closed: AtomicBool,
        paused: AtomicBool,
        pause_notify: TaskSignal,
        pause_epoch: AtomicUsize,
        pause_ack: AtomicUsize,
        pause_ack_notify: TaskSignal,
        batch_limit: usize,
        arena: Mutex<arena::Arena>,
    }

    pub(crate) struct PendingState {
        lock: Mutex<Option<Value>>,
        notify: TaskSignal,
        dropped: AtomicBool,
    }

    impl PoolQueue {
        pub fn new(cap: usize) -> Self {
            let cap = cap.max(2).next_power_of_two();
            let mut slots = Vec::with_capacity(cap);
            for i in 0..cap {
                slots.push(PoolSlot {
                    seq: AtomicUsize::new(i),
                    msg: UnsafeCell::new(MaybeUninit::uninit()),
                });
            }
            Self {
                cap,
                mask: cap - 1,
                head: AtomicUsize::new(0),
                tail: AtomicUsize::new(0),
                slots: slots.into_boxed_slice(),
            }
        }

        pub fn push(&self, msg: PoolMessage) -> Result<(), PoolMessage> {
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
                        unsafe {
                            (*slot.msg.get()).write(msg);
                        }
                        slot.seq.store(tail + 1, Ordering::Release);
                        return Ok(());
                    }
                } else if diff < 0 {
                    return Err(msg);
                }
                tail = self.tail.load(Ordering::Relaxed);
            }
        }

        pub fn pop(&self) -> Option<PoolMessage> {
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
                        let msg = unsafe { (*slot.msg.get()).assume_init_read() };
                        slot.seq.store(head + self.cap, Ordering::Release);
                        return Some(msg);
                    }
                } else if diff < 0 {
                    return None;
                }
                head = self.head.load(Ordering::Relaxed);
            }
        }

        pub fn has_more(&self) -> bool {
            self.len() > 0
        }

        pub fn len(&self) -> usize {
            let head = self.head.load(Ordering::Acquire);
            let tail = self.tail.load(Ordering::Acquire);
            tail.wrapping_sub(head)
        }

        #[cfg(test)]
        fn set_counters_for_test(&self, head: usize, tail: usize) {
            self.head.store(head, Ordering::Relaxed);
            self.tail.store(tail, Ordering::Relaxed);
        }
    }

    static METHODS: OnceLock<Mutex<HashMap<u32, HashMap<u32, MethodFn>>>> = OnceLock::new();
    static ARGS_POOL: OnceLock<Mutex<Vec<Vec<Value>>>> = OnceLock::new();
    static PENDING_POOL: OnceLock<Mutex<Vec<Arc<PendingState>>>> = OnceLock::new();
    static WATCHDOG_STARTED: OnceLock<()> = OnceLock::new();

    fn method_registry() -> &'static Mutex<HashMap<u32, HashMap<u32, MethodFn>>> {
        METHODS.get_or_init(|| Mutex::new(HashMap::new()))
    }

    fn args_pool() -> &'static Mutex<Vec<Vec<Value>>> {
        ARGS_POOL.get_or_init(|| Mutex::new(Vec::new()))
    }

    fn pending_pool() -> &'static Mutex<Vec<Arc<PendingState>>> {
        PENDING_POOL.get_or_init(|| Mutex::new(Vec::new()))
    }

    const ARGS_POOL_MAX_CAP: usize = 64;
    const ARGS_POOL_MAX_LEN: usize = 128;
    const PENDING_POOL_MAX_LEN: usize = 512;

    fn ensure_watchdog() {
        if WATCHDOG_STARTED.get().is_some() {
            return;
        }
        let ms = config::actor_watchdog_ms();
        if ms == 0 {
            return;
        }
        let _ = WATCHDOG_STARTED.set(());
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(ms));
            eprintln!("fatal: watchdog expired after {} ms", ms);
            std::process::abort();
        });
    }

    fn runtime_error(message: &str) {
        eprintln!("runtime error: {message}");
    }

    pub(crate) fn runtime_spawn<F>(task: F)
    where
        F: FnOnce() + Send + 'static,
    {
        ensure_watchdog();
        std::thread::spawn(task);
    }

    fn take_args_vec() -> Vec<Value> {
        let mut pool = args_pool().lock().expect("args pool lock");
        pool.pop().unwrap_or_default()
    }

    fn return_args_vec(mut vec: Vec<Value>) {
        if vec.capacity() > ARGS_POOL_MAX_CAP {
            return;
        }
        vec.clear();
        let mut pool = args_pool().lock().expect("args pool lock");
        if pool.len() < ARGS_POOL_MAX_LEN {
            pool.push(vec);
        }
    }

    fn take_pending_state() -> (Arc<PendingState>, bool) {
        let mut pool = pending_pool().lock().expect("pending pool lock");
        let Some(state) = pool.pop() else {
            return (
                Arc::new(PendingState {
                    lock: Mutex::new(None),
                    notify: TaskSignal::new(),
                    dropped: AtomicBool::new(false),
                }),
                true,
            );
        };
        drop(pool);
        {
            let mut guard = state.lock.lock().expect("pending lock");
            if let Some(val) = guard.take() {
                unsafe { wr_rc_dec(val) };
            }
        }
        state.dropped.store(false, Ordering::Release);
        (state, false)
    }

    fn recycle_pending_state(state: Arc<PendingState>) {
        if Arc::strong_count(&state) != 1 {
            return;
        }
        {
            let mut guard = state.lock.lock().expect("pending lock");
            if let Some(val) = guard.take() {
                unsafe { wr_rc_dec(val) };
            }
        }
        state.dropped.store(false, Ordering::Release);
        let mut pool = pending_pool().lock().expect("pending pool lock");
        if pool.len() < PENDING_POOL_MAX_LEN {
            pool.push(state);
        }
    }

    pub fn register_method(class_id: u32, method_id: u32, func: MethodFn) {
        let mut map = method_registry().lock().expect("method registry lock");
        map.entry(class_id)
            .or_insert_with(HashMap::new)
            .insert(method_id, func);
    }

    pub fn actor_spawn(
        class_id: u64,
        instance: Value,
        pool_size: i64,
        objective: i64,
        mailbox_cap: i64,
        enqueue_timeout_ms: i64,
        batch_limit: i64,
    ) -> Value {
        let class_id = class_id as u32;
        let Some(objective) = objective_u8(objective) else {
            runtime_error("actor_spawn: `objective` must be in [0, 3]");
            return Value::nil();
        };
        let mut config = config::actor_config();
        if mailbox_cap > 0 {
            config.mailbox_cap = mailbox_cap as usize;
        } else if mailbox_cap < 0 {
            if mailbox_cap != -1 {
                runtime_error("actor_spawn: `mailbox_cap` must be > 0 or -1 (use runtime config)");
                return Value::nil();
            }
        }
        if enqueue_timeout_ms >= 0 {
            config.enqueue_timeout = Duration::from_millis(enqueue_timeout_ms as u64);
        } else if enqueue_timeout_ms != -1 {
            runtime_error(
                "actor_spawn: `enqueue_timeout_ms` must be >= 0 or -1 (use runtime config)",
            );
            return Value::nil();
        }
        if batch_limit > 0 {
            config.batch_limit = batch_limit as usize;
        } else if batch_limit < 0 {
            if batch_limit != -1 {
                runtime_error("actor_spawn: `batch_limit` must be > 0 or -1 (use runtime config)");
                return Value::nil();
            }
        }
        if config.mailbox_cap == 0 || config.batch_limit == 0 {
            runtime_error(
                "actor_spawn: resolved runtime config is invalid (`mailbox_cap` and `batch_limit` must be > 0)",
            );
            return Value::nil();
        }
        let Some(pool_size) = (pool_size > 0).then_some(pool_size) else {
            runtime_error("actor_spawn: `pool_size` must be > 0");
            return Value::nil();
        };
        let (tx, rx) = mpsc::sync_channel(config.mailbox_cap);
        let mailbox = Arc::new(Mailbox {
            sender: Mutex::new(Some(tx)),
            len: AtomicUsize::new(0),
            closed: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            pause_notify: TaskSignal::new(),
            pause_epoch: AtomicUsize::new(0),
            pause_ack: AtomicUsize::new(0),
            pause_ack_notify: TaskSignal::new(),
            batch_limit: config.batch_limit,
            arena: Mutex::new(arena::Arena::new(64 * 1024)),
        });
        unsafe {
            wr_rc_inc(instance);
        }
        let mailbox_for_loop = mailbox.clone();
        runtime_spawn(move || actor_loop(class_id, mailbox_for_loop, rx));
        let obj = Box::new(ActorHandle {
            header: header(TypeId::Actor),
            class_id,
            pool_size,
            objective,
            mailbox,
            instance,
        });
        Value::from_ptr(Box::into_raw(obj) as *mut ObjHeader)
    }

    pub fn pool_new(
        handles_val: Value,
        objective: i64,
        min_size: i64,
        max_size: i64,
        weight: i64,
        queue_cap: i64,
    ) -> Value {
        let list = match crate::list::as_list_ref(handles_val) {
            Some(list) => list,
            None => return Value::nil(),
        };
        let Some(objective) = objective_u8(objective) else {
            runtime_error("pool_new: `objective` must be in [0, 3]");
            return Value::nil();
        };
        let Ok(min_size) = u32::try_from(min_size) else {
            runtime_error("pool_new: `min_size` must be >= 0");
            return Value::nil();
        };
        let Ok(max_size) = u32::try_from(max_size) else {
            runtime_error("pool_new: `max_size` must be >= 0");
            return Value::nil();
        };
        let Ok(weight) = u32::try_from(weight) else {
            runtime_error("pool_new: `weight` must be >= 0");
            return Value::nil();
        };
        if max_size != 0 && min_size > max_size {
            runtime_error("pool_new: `min_size` must be <= `max_size` unless `max_size` is 0");
            return Value::nil();
        }
        let mut handles = Vec::new();
        unsafe {
            for handle in (*list).data.iter() {
                wr_rc_inc(*handle);
                handles.push(*handle);
            }
        }
        let pool_size = handles.len();
        let drop_on_full = queue_cap == 0;
        let queue_cap = if queue_cap == 0 {
            1
        } else if queue_cap > 0 {
            queue_cap as usize
        } else {
            runtime_error("pool_new: `queue_cap` must be >= 0");
            return Value::nil();
        };
        let obj = Box::new(PoolHandle {
            header: header(TypeId::Pool),
            pool_id: 0,
            shard_id: 0,
            objective,
            pool_size,
            rr: AtomicUsize::new(0),
            handles,
            queue: PoolQueue::new(queue_cap),
            credits: AtomicI64::new(0),
            min_share: min_size,
            max_share: max_size,
            weight,
            has_ready: AtomicBool::new(false),
            alive: AtomicBool::new(true),
            enqueue_inflight: AtomicUsize::new(0),
            next_in_shard: AtomicUsize::new(0),
            batch_limit: config::sched_batch_limit(),
            drop_on_full,
            shard_hint: AtomicUsize::new(0),
        });
        let obj_ptr = Box::into_raw(obj);
        let (pool_id, shard_id) = scheduler::register_pool(obj_ptr);
        unsafe {
            (*obj_ptr).pool_id = pool_id;
            (*obj_ptr).shard_id = shard_id;
        }
        Value::from_ptr(obj_ptr as *mut ObjHeader)
    }

    fn objective_u8(objective: i64) -> Option<u8> {
        u8::try_from(objective)
            .ok()
            .filter(|objective| *objective <= 3)
    }

    pub fn pool_size(handle: Value) -> Value {
        if let Some(pool) = as_pool_ref(handle) {
            unsafe { return Value::from_int((*pool).pool_size as i64) };
        }
        if as_actor(handle).is_some() {
            return Value::from_int(1);
        }
        Value::nil()
    }

    pub fn pool_rr(handle: Value) -> Value {
        if let Some(pool) = as_pool_ref(handle) {
            unsafe { return Value::from_int((*pool).rr.load(Ordering::Relaxed) as i64) };
        }
        Value::nil()
    }

    pub fn pool_queue_len(handle: Value) -> Value {
        if let Some(pool) = as_pool_ref(handle) {
            unsafe { return Value::from_int((*pool).queue.len() as i64) };
        }
        Value::nil()
    }

    pub fn actor_mailbox_len(handle: Value) -> Value {
        if let Some(pool) = as_pool_ref(handle) {
            let mut total = 0usize;
            unsafe {
                for handle in (&(*pool).handles).iter() {
                    if let Some(actor) = as_actor(*handle) {
                        total += mailbox_len(actor);
                    }
                }
            }
            return Value::from_int(total as i64);
        }
        if let Some(actor) = as_actor(handle) {
            return Value::from_int(mailbox_len(actor) as i64);
        }
        Value::nil()
    }

    pub fn actor_pause(handle: Value) {
        if let Some(pool) = as_pool_ref(handle) {
            unsafe {
                for handle in (&(*pool).handles).iter() {
                    if let Some(actor) = as_actor(*handle) {
                        mailbox_set_paused(actor, true);
                        enqueue_pause_message(actor);
                    }
                }
            }
            return;
        }
        if let Some(actor) = as_actor(handle) {
            mailbox_set_paused(actor, true);
            enqueue_pause_message(actor);
        }
    }

    pub fn actor_resume(handle: Value) {
        if let Some(pool) = as_pool_ref(handle) {
            unsafe {
                for handle in (&(*pool).handles).iter() {
                    if let Some(actor) = as_actor(*handle) {
                        mailbox_set_paused(actor, false);
                    }
                }
            }
            return;
        }
        if let Some(actor) = as_actor(handle) {
            mailbox_set_paused(actor, false);
        }
    }

    pub fn actor_pause_wait(handle: Value) {
        if let Some(pool) = as_pool_ref(handle) {
            unsafe {
                for handle in (&(*pool).handles).iter() {
                    if let Some(actor) = as_actor(*handle) {
                        mailbox_wait_paused(actor);
                    }
                }
            }
            return;
        }
        if let Some(actor) = as_actor(handle) {
            mailbox_wait_paused(actor);
        }
    }

    pub fn actor_send(handle: Value, method_id: u32, argc: usize, argv_ptr: *const Value) -> Value {
        if let Some(pool) = as_pool_ref(handle) {
            return pool_send(pool, method_id, argc, argv_ptr);
        }
        let actor = match as_actor(handle) {
            Some(actor) => actor,
            None => return Value::nil(),
        };
        if mailbox_should_drop_paused(actor) {
            inc_messages_dropped_paused();
            return Value::nil();
        }
        let (state, allocated) = take_pending_state();
        let Some(msg) = build_message_local(actor, method_id, argc, argv_ptr, Some(state.clone()))
        else {
            recycle_pending_state(state);
            return Value::nil();
        };
        #[cfg(feature = "metrics")]
        if allocated {
            inc_alloc_pending();
        }
        let pending = Box::new(PendingObj {
            header: header(TypeId::Pending),
            state,
        });
        unsafe {
            enqueue_message_ref(&*(*actor).mailbox, msg);
        }
        Value::from_ptr(Box::into_raw(pending) as *mut ObjHeader)
    }

    pub fn sleep_ms(ms: i64) -> Value {
        let (val, state) = pending_new();
        if ms <= 0 {
            resolve_pending(state, Value::nil());
            return val;
        }
        runtime_spawn(move || {
            std::thread::sleep(Duration::from_millis(ms as u64));
            resolve_pending(state, Value::nil());
        });
        val
    }

    pub fn actor_fire(handle: Value, method_id: u32, argc: usize, argv_ptr: *const Value) {
        if let Some(pool) = as_pool_ref(handle) {
            pool_fire(pool, method_id, argc, argv_ptr);
            return;
        }
        let actor = match as_actor(handle) {
            Some(actor) => actor,
            None => return,
        };
        if mailbox_should_drop_paused(actor) {
            inc_messages_dropped_paused();
            return;
        }
        let Some(msg) = build_message_local(actor, method_id, argc, argv_ptr, None) else {
            return;
        };
        unsafe {
            enqueue_message_ref(&*(*actor).mailbox, msg);
        }
    }

    pub fn deliver_pool_message(msg: PoolMessage) {
        enqueue_message(msg.mailbox, msg.msg);
    }

    pub fn pending_await(pending: Value) -> Value {
        if !pending.is_ptr() {
            return Value::nil();
        }
        unsafe {
            let header = &*pending.as_ptr();
            if header.type_id != TypeId::Pending as u32 {
                return Value::nil();
            }
            let p = &*(pending.as_ptr() as *const PendingObj);
            loop {
                let guard = p.state.lock.lock().expect("pending lock");
                if let Some(val) = *guard {
                    return result::result_ok(val);
                }
                drop(guard);
                let observed_epoch = p.state.notify.snapshot();
                let _ = p.state.notify.wait(observed_epoch);
            }
        }
    }

    pub unsafe fn drop_actor(ptr: *mut ObjHeader) {
        let actor = ptr as *mut ActorHandle;
        unsafe {
            let mailbox = (*actor).mailbox.clone();
            mailbox.closed.store(true, Ordering::Release);
            let sender = {
                let mut guard = mailbox.sender.lock().expect("mailbox sender lock");
                guard.take()
            };
            drop(sender);
            // Keep actor allocation/instance alive for process lifetime to avoid
            // teardown races with concurrent raw-pointer readers.
        }
    }

    pub unsafe fn drop_pool(ptr: *mut ObjHeader) {
        let pool = ptr as *mut PoolHandle;
        unsafe {
            crate::diagnostics::log_event("pool_drop");
            (*pool).alive.store(false, Ordering::Release);
            scheduler::retire_pool(pool as *const PoolHandle);
        }
    }

    pub unsafe fn drop_pending(ptr: *mut ObjHeader) {
        let pending = unsafe { Box::from_raw(ptr as *mut PendingObj) };
        let state = pending.state.clone();
        state.dropped.store(true, Ordering::Release);
        inc_pending_dropped();
        let mut guard = state.lock.lock().expect("pending lock");
        if let Some(val) = guard.take() {
            unsafe { wr_rc_dec(val) };
        }
        drop(guard);
        recycle_pending_state(state);
    }

    fn as_actor(val: Value) -> Option<*const ActorHandle> {
        if !val.is_ptr() {
            return None;
        }
        unsafe {
            let header = &*val.as_ptr();
            if header.type_id != TypeId::Actor as u32 {
                return None;
            }
            Some(val.as_ptr() as *const ActorHandle)
        }
    }

    fn mailbox_len(actor: *const ActorHandle) -> usize {
        unsafe { (&(*actor).mailbox).len.load(Ordering::Relaxed) }
    }

    fn mailbox_dec(mailbox: &Mailbox) {
        let mut current = mailbox.len.load(Ordering::Acquire);
        loop {
            if current == 0 {
                return;
            }
            match mailbox.len.compare_exchange(
                current,
                current - 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    inc_mailbox_dequeue();
                    return;
                }
                Err(next) => current = next,
            }
        }
    }

    fn mailbox_set_paused(actor: *const ActorHandle, paused: bool) {
        unsafe {
            let mailbox = &(*actor).mailbox;
            if paused {
                mailbox.pause_epoch.fetch_add(1, Ordering::AcqRel);
            }
            mailbox.paused.store(paused, Ordering::Release);
            if !paused {
                mailbox.pause_notify.notify_waiters();
            }
        }
    }

    unsafe fn args_from_raw<'a>(argc: usize, argv_ptr: *const Value) -> Option<&'a [Value]> {
        if argc == 0 {
            return Some(&[]);
        }
        if argv_ptr.is_null() {
            return None;
        }
        Some(unsafe { std::slice::from_raw_parts(argv_ptr, argc) })
    }

    fn build_message_inner(
        instance: Value,
        method_id: u32,
        args: &[Value],
        pending: Option<Arc<PendingState>>,
    ) -> Option<Message> {
        let mut args_vec = take_args_vec();
        args_vec.push(instance);
        if !args.is_empty() {
            args_vec.extend_from_slice(args);
        }
        for arg in &args_vec {
            if arena::is_arena_value(*arg) {
                return_args_vec(args_vec);
                return None;
            }
        }
        let msg = Message {
            method_id,
            args: args_vec,
            pending,
        };
        unsafe {
            for arg in &msg.args {
                wr_rc_inc(*arg);
            }
        }
        Some(msg)
    }

    fn build_message_local(
        actor: *const ActorHandle,
        method_id: u32,
        argc: usize,
        argv_ptr: *const Value,
        pending: Option<Arc<PendingState>>,
    ) -> Option<Message> {
        let instance = unsafe { (*actor).instance };
        let args = unsafe { args_from_raw(argc, argv_ptr)? };
        build_message_inner(instance, method_id, args, pending)
    }

    fn build_pool_message(
        actor: *const ActorHandle,
        method_id: u32,
        argc: usize,
        argv_ptr: *const Value,
        pending: Option<Arc<PendingState>>,
    ) -> Option<PoolMessage> {
        let mailbox = unsafe { (*actor).mailbox.clone() };
        let instance = unsafe { (*actor).instance };
        let args = unsafe { args_from_raw(argc, argv_ptr)? };
        let msg = build_message_inner(instance, method_id, args, pending)?;
        Some(PoolMessage { mailbox, msg })
    }

    fn mailbox_should_drop_paused(actor: *const ActorHandle) -> bool {
        unsafe {
            let mailbox = &(*actor).mailbox;
            if !mailbox.paused.load(Ordering::Acquire) {
                return false;
            }
            let cap = crate::config::pause_queue_cap();
            mailbox.len.load(Ordering::Acquire) >= cap
        }
    }

    fn mailbox_wait_paused(actor: *const ActorHandle) {
        unsafe {
            let mailbox = &(*actor).mailbox;
            let epoch = mailbox.pause_epoch.load(Ordering::Acquire);
            while mailbox.pause_ack.load(Ordering::Acquire) < epoch {
                let _ = mailbox
                    .pause_ack_notify
                    .wait(mailbox.pause_ack_notify.snapshot());
            }
        }
    }

    fn enqueue_pause_message(actor: *const ActorHandle) {
        unsafe {
            let args_vec = take_args_vec();
            let msg = Message {
                method_id: u32::MAX,
                args: args_vec,
                pending: None,
            };
            enqueue_message_ref(&*(*actor).mailbox, msg);
        }
    }

    fn as_pool_ref(val: Value) -> Option<*const PoolHandle> {
        if !val.is_ptr() {
            return None;
        }
        unsafe {
            let header = &*val.as_ptr();
            if header.type_id != TypeId::Pool as u32 {
                return None;
            }
            Some(val.as_ptr() as *const PoolHandle)
        }
    }

    fn pool_send(
        pool: *const PoolHandle,
        method_id: u32,
        argc: usize,
        argv_ptr: *const Value,
    ) -> Value {
        unsafe {
            if (*pool).pool_size == 0 {
                return Value::nil();
            }
            let idx = (*pool).rr.fetch_add(1, Ordering::Relaxed) % (*pool).pool_size;
            let handle = (&(*pool).handles)[idx];
            let actor = match as_actor(handle) {
                Some(actor) => actor,
                None => return Value::nil(),
            };
            if mailbox_should_drop_paused(actor) {
                inc_messages_dropped_paused();
                return Value::nil();
            }
            let (state, allocated) = take_pending_state();

            if (*pool).pool_size == 1 && !(*pool).drop_on_full && (*pool).queue.len() == 0 {
                let Some(msg) =
                    build_message_local(actor, method_id, argc, argv_ptr, Some(state.clone()))
                else {
                    recycle_pending_state(state);
                    return Value::nil();
                };
                #[cfg(feature = "metrics")]
                if allocated {
                    inc_alloc_pending();
                }
                let pending = Box::new(PendingObj {
                    header: header(TypeId::Pending),
                    state,
                });
                enqueue_message_ref(&*(*actor).mailbox, msg);
                return Value::from_ptr(Box::into_raw(pending) as *mut ObjHeader);
            }

            let Some(msg) =
                build_pool_message(actor, method_id, argc, argv_ptr, Some(state.clone()))
            else {
                recycle_pending_state(state);
                return Value::nil();
            };

            if let Err(msg) = scheduler::enqueue(pool, msg) {
                drop_message(msg.msg);
                recycle_pending_state(state);
                return Value::nil();
            }

            #[cfg(feature = "metrics")]
            if allocated {
                inc_alloc_pending();
            }
            let pending = Box::new(PendingObj {
                header: header(TypeId::Pending),
                state,
            });
            Value::from_ptr(Box::into_raw(pending) as *mut ObjHeader)
        }
    }

    fn pool_fire(pool: *const PoolHandle, method_id: u32, argc: usize, argv_ptr: *const Value) {
        unsafe {
            if (*pool).pool_size == 0 {
                return;
            }
            let idx = (*pool).rr.fetch_add(1, Ordering::Relaxed) % (*pool).pool_size;
            let handle = (&(*pool).handles)[idx];
            let actor = match as_actor(handle) {
                Some(actor) => actor,
                None => return,
            };
            if mailbox_should_drop_paused(actor) {
                inc_messages_dropped_paused();
                return;
            }
            if (*pool).pool_size == 1 && !(*pool).drop_on_full && (*pool).queue.len() == 0 {
                let Some(msg) = build_message_local(actor, method_id, argc, argv_ptr, None) else {
                    return;
                };
                enqueue_message_ref(&*(*actor).mailbox, msg);
                return;
            }

            if let Some(msg) = build_pool_message(actor, method_id, argc, argv_ptr, None) {
                if let Err(msg) = scheduler::enqueue(pool, msg) {
                    drop_message(msg.msg);
                }
            }
        }
    }

    fn enqueue_message(mailbox: Arc<Mailbox>, msg: Message) {
        enqueue_message_ref(&mailbox, msg);
    }

    fn enqueue_message_ref(mailbox: &Mailbox, msg: Message) {
        if mailbox.closed.load(Ordering::Acquire) {
            drop_message(msg);
            inc_mailbox_enqueue_fail();
            inc_messages_dropped();
            return;
        }
        let sender = {
            let guard = mailbox.sender.lock().expect("mailbox sender lock");
            guard.clone()
        };
        let Some(sender) = sender else {
            drop_message(msg);
            inc_mailbox_enqueue_fail();
            inc_messages_dropped();
            return;
        };
        let mut current = msg;
        loop {
            match sender.try_send(current) {
                Ok(()) => {
                    let len = mailbox.len.fetch_add(1, Ordering::AcqRel) + 1;
                    update_mailbox_high_water(len);
                    inc_mailbox_enqueue_ok();
                    inc_messages_sent();
                    return;
                }
                Err(TrySendError::Disconnected(msg)) => {
                    drop_message(msg);
                    inc_mailbox_enqueue_fail();
                    inc_messages_dropped();
                    return;
                }
                Err(TrySendError::Full(msg)) => {
                    current = msg;
                    match sender.send(current) {
                        Ok(()) => {
                            let len = mailbox.len.fetch_add(1, Ordering::AcqRel) + 1;
                            update_mailbox_high_water(len);
                            inc_mailbox_enqueue_ok();
                            inc_messages_sent();
                            return;
                        }
                        Err(err) => {
                            drop_message(err.0);
                            inc_mailbox_enqueue_fail();
                            inc_messages_dropped();
                            return;
                        }
                    }
                }
            }
        }
    }

    fn actor_loop(class_id: u32, mailbox: Arc<Mailbox>, rx: mpsc::Receiver<Message>) {
        loop {
            let msg = match rx.recv() {
                Ok(msg) => msg,
                Err(_) => break,
            };
            while mailbox.paused.load(Ordering::Acquire) {
                let epoch = mailbox.pause_epoch.load(Ordering::Acquire);
                mailbox.pause_ack.store(epoch, Ordering::Release);
                mailbox.pause_ack_notify.notify_waiters();
                let _ = mailbox.pause_notify.wait(mailbox.pause_notify.snapshot());
            }
            if handle_message(&mailbox, class_id, msg, &rx) {
                std::thread::yield_now();
            } else {
                break;
            }
        }
    }

    fn handle_message(
        mailbox: &Mailbox,
        class_id: u32,
        first: Message,
        rx: &mpsc::Receiver<Message>,
    ) -> bool {
        let batch_limit = mailbox.batch_limit;
        let mut current = first;
        for idx in 0..batch_limit {
            if mailbox.closed.load(Ordering::Acquire) {
                drop_message(current);
                mailbox_dec(mailbox);
                drain_messages(mailbox, rx);
                return false;
            }
            process_message(mailbox, class_id, current);
            mailbox_dec(mailbox);
            if idx + 1 >= batch_limit {
                break;
            }
            match rx.try_recv() {
                Ok(next) => current = next,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return false,
            }
        }
        true
    }

    fn drain_messages(mailbox: &Mailbox, rx: &mpsc::Receiver<Message>) {
        loop {
            let msg = match rx.try_recv() {
                Ok(msg) => msg,
                Err(_) => break,
            };
            drop_message(msg);
            mailbox_dec(mailbox);
        }
    }

    fn process_message(mailbox: &Mailbox, class_id: u32, msg: Message) {
        let mut arena_guard = mailbox.arena.lock().expect("arena lock");
        let _guard = arena::enter(&mut *arena_guard as *mut _);
        let func = {
            let map = method_registry().lock().expect("method registry lock");
            map.get(&class_id)
                .and_then(|inner| inner.get(&msg.method_id))
                .copied()
        };
        if func.is_none() && crate::config::debug_actor_enabled() {
            eprintln!(
                "actor: missing method class_id={} method_id={} argc={}",
                class_id,
                msg.method_id,
                msg.args.len()
            );
        }
        let result = if let Some(func) = func {
            let call = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                func(msg.args.len(), msg.args.as_ptr())
            }));
            match call {
                Ok(val) => val,
                Err(_) => Value::nil(),
            }
        } else {
            Value::nil()
        };
        let result = match arena::reject_arena_escape(result, "actor return") {
            Some(value) => value,
            None => Value::nil(),
        };
        if crate::config::debug_actor_enabled() {
            eprintln!("actor: method result raw={}", result.0);
        }
        if let Some(pending) = msg.pending {
            resolve_pending(pending, result);
        }
        if arena_guard.live() != 0 && crate::config::debug_actor_enabled() {
            eprintln!("arena: live objects after message = {}", arena_guard.live());
        }
        arena_guard.reset();
        unsafe {
            for arg in msg.args.iter().copied() {
                wr_rc_dec(arg);
            }
        }
        return_args_vec(msg.args);
    }

    fn drop_message(msg: Message) {
        if let Some(pending) = msg.pending {
            resolve_pending(pending, Value::nil());
        }
        unsafe {
            for arg in msg.args.iter().copied() {
                wr_rc_dec(arg);
            }
        }
        return_args_vec(msg.args);
    }

    pub(crate) fn resolve_pending(pending: Arc<PendingState>, value: Value) {
        if pending.dropped.load(Ordering::Acquire) {
            unsafe { wr_rc_dec(value) };
            recycle_pending_state(pending);
            return;
        }
        let mut guard = pending.lock.lock().expect("pending lock");
        if pending.dropped.load(Ordering::Acquire) {
            drop(guard);
            unsafe { wr_rc_dec(value) };
            recycle_pending_state(pending);
            return;
        }
        *guard = Some(value);
        inc_pending_resolved();
        pending.notify.notify_waiters();
    }

    pub(crate) fn pending_new() -> (Value, Arc<PendingState>) {
        let (state, allocated) = take_pending_state();
        let pending = Box::new(PendingObj {
            header: header(TypeId::Pending),
            state: state.clone(),
        });
        #[cfg(feature = "metrics")]
        if allocated {
            inc_alloc_pending();
        }
        let val = Value::from_ptr(Box::into_raw(pending) as *mut ObjHeader);
        (val, state)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::metrics;
        use std::sync::{Arc, Mutex, OnceLock};
        use std::thread;

        fn metrics_test_lock() -> &'static Mutex<()> {
            static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
            LOCK.get_or_init(|| Mutex::new(()))
        }

        fn dummy_mailbox() -> Arc<Mailbox> {
            let (tx, _rx) = mpsc::sync_channel(1);
            Arc::new(Mailbox {
                sender: Mutex::new(Some(tx)),
                len: AtomicUsize::new(0),
                closed: AtomicBool::new(false),
                paused: AtomicBool::new(false),
                pause_notify: TaskSignal::new(),
                pause_epoch: AtomicUsize::new(0),
                pause_ack: AtomicUsize::new(0),
                pause_ack_notify: TaskSignal::new(),
                batch_limit: 1,
                arena: Mutex::new(arena::Arena::new(1024)),
            })
        }

        fn test_mailbox() -> (Arc<Mailbox>, mpsc::Receiver<Message>) {
            let (tx, rx) = mpsc::sync_channel(1);
            let mailbox = Arc::new(Mailbox {
                sender: Mutex::new(Some(tx)),
                len: AtomicUsize::new(0),
                closed: AtomicBool::new(false),
                paused: AtomicBool::new(false),
                pause_notify: TaskSignal::new(),
                pause_epoch: AtomicUsize::new(0),
                pause_ack: AtomicUsize::new(0),
                pause_ack_notify: TaskSignal::new(),
                batch_limit: 1,
                arena: Mutex::new(arena::Arena::new(1024)),
            });
            (mailbox, rx)
        }

        #[cfg(feature = "metrics")]
        #[test]
        fn mailbox_metrics_enqueue_dequeue() {
            let _guard = metrics_test_lock().lock().expect("metrics test lock");
            metrics::reset();
            let (mailbox, rx) = test_mailbox();

            let before_ok = metrics::metrics_get_raw(metrics::METRIC_MAILBOX_ENQUEUE_OK);
            let before_dequeue = metrics::metrics_get_raw(metrics::METRIC_MAILBOX_DEQUEUE);

            let msg = Message {
                method_id: 0,
                args: take_args_vec(),
                pending: None,
            };
            enqueue_message(mailbox.clone(), msg);

            let after_ok = metrics::metrics_get_raw(metrics::METRIC_MAILBOX_ENQUEUE_OK);
            assert!(
                after_ok > before_ok,
                "enqueue metric did not advance (before={before_ok}, after={after_ok})"
            );

            let received = rx.recv().expect("recv message");
            drop_message(received);
            mailbox_dec(mailbox.as_ref());

            let after_dequeue = metrics::metrics_get_raw(metrics::METRIC_MAILBOX_DEQUEUE);
            assert!(
                after_dequeue > before_dequeue,
                "dequeue metric did not advance (before={before_dequeue}, after={after_dequeue})"
            );
        }

        #[cfg(feature = "metrics")]
        #[test]
        fn mailbox_metrics_enqueue_fail() {
            let _guard = metrics_test_lock().lock().expect("metrics test lock");
            metrics::reset();
            let (mailbox, _rx) = test_mailbox();
            mailbox.closed.store(true, Ordering::Release);

            let before_fail = metrics::metrics_get_raw(metrics::METRIC_MAILBOX_ENQUEUE_FAIL);
            let msg = Message {
                method_id: 0,
                args: take_args_vec(),
                pending: None,
            };
            enqueue_message(mailbox, msg);
            let after_fail = metrics::metrics_get_raw(metrics::METRIC_MAILBOX_ENQUEUE_FAIL);
            assert!(
                after_fail > before_fail,
                "enqueue-fail metric did not advance (before={before_fail}, after={after_fail})"
            );
        }

        #[test]
        fn pool_queue_mpsc_multi_producer() {
            let queue = Arc::new(PoolQueue::new(8));
            let mailbox = dummy_mailbox();
            let producers = 4usize;
            let per_producer = 200usize;
            let total = producers * per_producer;

            let mut handles = Vec::new();
            for _ in 0..producers {
                let queue = queue.clone();
                let mailbox = mailbox.clone();
                handles.push(thread::spawn(move || {
                    for _ in 0..per_producer {
                        let mut msg = PoolMessage {
                            mailbox: mailbox.clone(),
                            msg: Message {
                                method_id: 0,
                                args: Vec::new(),
                                pending: None,
                            },
                        };
                        loop {
                            match queue.push(msg) {
                                Ok(()) => break,
                                Err(returned) => {
                                    msg = returned;
                                    thread::yield_now();
                                }
                            }
                        }
                    }
                }));
            }

            let mut received = 0usize;
            while received < total {
                if queue.pop().is_some() {
                    received += 1;
                } else {
                    thread::yield_now();
                }
            }

            for handle in handles {
                handle.join().expect("producer join");
            }
            assert_eq!(queue.len(), 0);
        }

        #[test]
        fn pool_queue_len_wraparound() {
            let queue = PoolQueue::new(8);
            let head = usize::MAX - 5;
            let tail = 3usize;
            queue.set_counters_for_test(head, tail);
            let expected = tail.wrapping_sub(head);
            assert_eq!(queue.len(), expected);
            assert_eq!(queue.has_more(), expected > 0);
        }

        #[test]
        #[ignore]
        fn pool_queue_mpsc_perf_sanity() {
            let queue = Arc::new(PoolQueue::new(1024));
            let mailbox = dummy_mailbox();
            let producers = 4usize;
            let per_producer = 200_000usize;
            let total = producers * per_producer;

            let mut handles = Vec::new();
            for _ in 0..producers {
                let queue = queue.clone();
                let mailbox = mailbox.clone();
                handles.push(thread::spawn(move || {
                    for _ in 0..per_producer {
                        let mut msg = PoolMessage {
                            mailbox: mailbox.clone(),
                            msg: Message {
                                method_id: 0,
                                args: Vec::new(),
                                pending: None,
                            },
                        };
                        loop {
                            match queue.push(msg) {
                                Ok(()) => break,
                                Err(returned) => {
                                    msg = returned;
                                    thread::yield_now();
                                }
                            }
                        }
                    }
                }));
            }

            let mut received = 0usize;
            while received < total {
                if queue.pop().is_some() {
                    received += 1;
                } else {
                    thread::yield_now();
                }
            }

            for handle in handles {
                handle.join().expect("producer join");
            }
            assert_eq!(queue.len(), 0);
        }

        #[test]
        fn enqueue_after_retire_increments_metric() {
            let _guard = metrics_test_lock().lock().expect("metrics test lock");
            metrics::reset();
            let actor_handle = actor_spawn(42, Value::nil(), 1, 3, 256, 10, 64);
            let handles = crate::list::list_new(0);
            crate::list::list_push(handles, actor_handle);
            let pool = pool_new(handles, 0, 0, 0, 0, 256);
            let pool_ptr = pool.as_ptr() as *const PoolHandle;
            unsafe {
                (*pool_ptr).alive.store(false, Ordering::Release);
            }
            let mailbox = dummy_mailbox();
            let msg = PoolMessage {
                mailbox,
                msg: Message {
                    method_id: 0,
                    args: Vec::new(),
                    pending: None,
                },
            };
            let _ = crate::scheduler::enqueue(pool_ptr, msg);
            assert!(metrics::get(metrics::METRIC_POOL_ENQUEUE_AFTER_RETIRE) >= 1);
            unsafe {
                wr_rc_dec(pool);
                wr_rc_dec(handles);
                wr_rc_dec(actor_handle);
            }
        }
    }
}

pub(crate) mod config {
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;

    use crate::class::class_get;
    use crate::value::{Value, int_value};
    use crate::wr_rc_dec;

    #[derive(Clone, Copy)]
    pub struct ActorConfig {
        pub mailbox_cap: usize,
        pub enqueue_timeout: Duration,
        pub batch_limit: usize,
    }

    #[derive(Clone)]
    pub struct RuntimeConfig {
        pub actor_mailbox_cap: usize,
        pub actor_enqueue_timeout_ms: u64,
        pub actor_batch_limit: usize,
        pub pause_queue_cap: usize,
        pub deterministic: bool,
        pub debug_actor: bool,
        pub actor_watchdog_ms: u64,
        pub sched_watchdog_ms: u64,
        pub sched_shards: usize,
        pub sched_tick_ms: u64,
        pub sched_ready_cap: usize,
        pub sched_batch_limit: i64,
        pub reactor_disable_io_uring: bool,
        pub pool_min_share: u32,
        pub pool_max_share: u32,
        pub pool_queue_cap: usize,
        pub pool_auto_min: i64,
        pub pool_auto_max: i64,
        pub diagnostics_enabled: bool,
    }

    impl Default for RuntimeConfig {
        fn default() -> Self {
            let cores = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1);
            Self {
                actor_mailbox_cap: 256,
                actor_enqueue_timeout_ms: 10,
                actor_batch_limit: 64,
                pause_queue_cap: 128,
                deterministic: false,
                debug_actor: false,
                actor_watchdog_ms: 0,
                sched_watchdog_ms: 0,
                sched_shards: cores,
                sched_tick_ms: 1,
                sched_ready_cap: 1024,
                sched_batch_limit: 64,
                reactor_disable_io_uring: false,
                pool_min_share: 1,
                pool_max_share: (cores * 2).max(1) as u32,
                pool_queue_cap: 256,
                pool_auto_min: 1,
                pool_auto_max: cores as i64,
                diagnostics_enabled: false,
            }
        }
    }

    static RUNTIME_CONFIG: OnceLock<Mutex<RuntimeConfig>> = OnceLock::new();

    fn runtime_config() -> RuntimeConfig {
        RUNTIME_CONFIG
            .get_or_init(|| Mutex::new(RuntimeConfig::default()))
            .lock()
            .expect("runtime config lock")
            .clone()
    }

    fn set_runtime_config(config: RuntimeConfig) {
        *RUNTIME_CONFIG
            .get_or_init(|| Mutex::new(RuntimeConfig::default()))
            .lock()
            .expect("runtime config lock") = config;
    }

    pub fn runtime_configure(config: Value) -> Value {
        let runtime_config = runtime_config_from_value(config);
        validate_runtime_config(&runtime_config);
        set_runtime_config(runtime_config);
        Value::nil()
    }

    fn runtime_config_from_value(config: Value) -> RuntimeConfig {
        let mut out = RuntimeConfig::default();
        if let Some(val) = config_field_usize(config, "actor_mailbox_cap") {
            out.actor_mailbox_cap = val;
        }
        if let Some(val) = config_field_u64(config, "actor_enqueue_timeout_ms") {
            out.actor_enqueue_timeout_ms = val;
        }
        if let Some(val) = config_field_usize(config, "actor_batch_limit") {
            out.actor_batch_limit = val;
        }
        if let Some(val) = config_field_usize(config, "pause_queue_cap") {
            out.pause_queue_cap = val;
        }
        if let Some(val) = config_field_bool(config, "deterministic") {
            out.deterministic = val;
        }
        if let Some(val) = config_field_bool(config, "debug_actor") {
            out.debug_actor = val;
        }
        if let Some(val) = config_field_u64(config, "actor_watchdog_ms") {
            out.actor_watchdog_ms = val;
        }
        if let Some(val) = config_field_u64(config, "sched_watchdog_ms") {
            out.sched_watchdog_ms = val;
        }
        if let Some(val) = config_field_usize(config, "sched_shards") {
            out.sched_shards = val;
        }
        if let Some(val) = config_field_u64(config, "sched_tick_ms") {
            out.sched_tick_ms = val;
        }
        if let Some(val) = config_field_usize(config, "sched_ready_cap") {
            out.sched_ready_cap = val;
        }
        if let Some(val) = config_field_i64(config, "sched_batch_limit") {
            out.sched_batch_limit = val;
        }
        if let Some(val) = config_field_bool(config, "reactor_disable_io_uring") {
            out.reactor_disable_io_uring = val;
        }
        if let Some(val) = config_field_u32(config, "pool_min_share") {
            out.pool_min_share = val;
        }
        if let Some(val) = config_field_u32(config, "pool_max_share") {
            out.pool_max_share = val;
        }
        if let Some(val) = config_field_usize(config, "pool_queue_cap") {
            out.pool_queue_cap = val;
        }
        if let Some(val) = config_field_i64(config, "pool_auto_min") {
            out.pool_auto_min = val;
        }
        if let Some(val) = config_field_i64(config, "pool_auto_max") {
            out.pool_auto_max = val;
        }
        if let Some(val) = config_field_bool(config, "diagnostics_enabled") {
            out.diagnostics_enabled = val;
        }
        out
    }

    fn config_field_bool(config: Value, field: &str) -> Option<bool> {
        let val = class_get(config, field.as_ptr(), field.len());
        if val.is_nil() {
            unsafe { wr_rc_dec(val) };
            return None;
        }
        let out = if val.is_bool() {
            Some(val.as_bool())
        } else {
            panic!("runtime_configure: field `{field}` must be a Boolean");
        };
        unsafe { wr_rc_dec(val) };
        out
    }

    fn config_field_usize(config: Value, field: &str) -> Option<usize> {
        config_field_i64(config, field).map(|val| {
            usize::try_from(val).unwrap_or_else(|_| {
                panic!("runtime_configure: field `{field}` must be a non-negative integer")
            })
        })
    }

    fn config_field_u64(config: Value, field: &str) -> Option<u64> {
        config_field_i64(config, field).map(|val| {
            u64::try_from(val).unwrap_or_else(|_| {
                panic!("runtime_configure: field `{field}` must be a non-negative integer")
            })
        })
    }

    fn config_field_u32(config: Value, field: &str) -> Option<u32> {
        config_field_i64(config, field).map(|val| {
            u32::try_from(val).unwrap_or_else(|_| {
                panic!("runtime_configure: field `{field}` must be in u32 range")
            })
        })
    }

    fn config_field_i64(config: Value, field: &str) -> Option<i64> {
        let val = class_get(config, field.as_ptr(), field.len());
        if val.is_nil() {
            unsafe { wr_rc_dec(val) };
            return None;
        }
        let out = int_value(val)
            .or_else(|| panic!("runtime_configure: field `{field}` must be an Integer"));
        unsafe { wr_rc_dec(val) };
        out
    }

    pub fn actor_config() -> ActorConfig {
        let config = runtime_config();
        ActorConfig {
            mailbox_cap: config.actor_mailbox_cap,
            enqueue_timeout: Duration::from_millis(config.actor_enqueue_timeout_ms),
            batch_limit: config.actor_batch_limit,
        }
    }

    pub fn pause_queue_cap() -> usize {
        runtime_config().pause_queue_cap
    }

    pub fn debug_actor_enabled() -> bool {
        runtime_config().debug_actor
    }

    pub fn actor_watchdog_ms() -> u64 {
        runtime_config().actor_watchdog_ms
    }

    pub fn sched_watchdog_ms() -> u64 {
        runtime_config().sched_watchdog_ms
    }

    pub fn sched_shards() -> usize {
        runtime_config().sched_shards
    }

    pub fn sched_tick_ms() -> u64 {
        runtime_config().sched_tick_ms
    }

    pub fn sched_ready_cap() -> usize {
        runtime_config().sched_ready_cap
    }

    pub fn sched_batch_limit() -> i64 {
        runtime_config().sched_batch_limit
    }

    pub fn reactor_disable_io_uring() -> bool {
        runtime_config().reactor_disable_io_uring
    }

    fn validate_runtime_config(config: &RuntimeConfig) {
        assert!(
            config.actor_mailbox_cap > 0,
            "runtime config `actor_mailbox_cap` must be > 0"
        );
        assert!(
            config.actor_enqueue_timeout_ms > 0,
            "runtime config `actor_enqueue_timeout_ms` must be > 0"
        );
        assert!(
            config.actor_batch_limit > 0,
            "runtime config `actor_batch_limit` must be > 0"
        );
        assert!(
            config.pause_queue_cap > 0,
            "runtime config `pause_queue_cap` must be > 0"
        );
        assert!(
            config.sched_shards > 0,
            "runtime config `sched_shards` must be > 0"
        );
        assert!(
            config.sched_tick_ms > 0,
            "runtime config `sched_tick_ms` must be > 0"
        );
        assert!(
            config.sched_ready_cap >= 2,
            "runtime config `sched_ready_cap` must be >= 2"
        );
        assert!(
            config.sched_batch_limit > 0,
            "runtime config `sched_batch_limit` must be > 0"
        );
        assert!(
            config.pool_min_share > 0,
            "runtime config `pool_min_share` must be > 0"
        );
        assert!(
            config.pool_max_share > 0,
            "runtime config `pool_max_share` must be > 0"
        );
        assert!(
            config.pool_queue_cap > 0,
            "runtime config `pool_queue_cap` must be > 0"
        );
    }
}

pub(crate) mod diagnostics {
    use std::sync::Once;

    pub const RUNTIME_ABI_VERSION: u32 = 4;

    static INIT: Once = Once::new();

    pub fn runtime_init() {
        INIT.call_once(|| {});
    }

    pub fn dump_diagnostics() {
        // Intentionally minimal after runtime module cleanup.
    }

    pub fn log_event(_event: &str) {
        // Intentionally minimal after runtime module cleanup.
    }
}

pub(crate) mod metrics {
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
}

pub(crate) mod scheduler {
    use crate::actor::{PoolHandle, PoolMessage, deliver_pool_message};
    use crate::config::{sched_ready_cap, sched_shards, sched_tick_ms};
    use crate::diagnostics;
    use crate::metrics::{
        inc_pool_enqueue_after_retire, inc_pool_queue_full, inc_sched_dispatched,
    };
    use crate::reactor::task::TaskSignal;
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex, OnceLock};
    use std::time::{Duration, Instant};

    static SHARDS: OnceLock<Vec<Arc<SchedulerShard>>> = OnceLock::new();
    static POOL_COUNTER: AtomicU64 = AtomicU64::new(1);

    pub struct SchedulerShard {
        ready: ReadyQueue,
        head: AtomicUsize,
        notify: TaskSignal,
        has_work: AtomicBool,
        retired: Mutex<Vec<usize>>,
    }

    impl SchedulerShard {
        fn new(_id: usize) -> Self {
            Self {
                ready: ReadyQueue::new(sched_ready_cap()),
                head: AtomicUsize::new(0),
                notify: TaskSignal::new(),
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
                crate::actor::runtime_spawn(move || scheduler_loop(shard));
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

    fn scheduler_loop(shard: Arc<SchedulerShard>) {
        let tick = Duration::from_millis(sched_tick_ms());
        let mut last_progress = Instant::now();
        let watchdog_ms = crate::config::sched_watchdog_ms();
        loop {
            if !shard.has_work.load(Ordering::Acquire) {
                let observed_epoch = shard.notify.snapshot();
                let _ = shard.notify.wait_timeout(observed_epoch, tick);
            }
            let dispatched = dispatch_ready(&shard);
            if dispatched > 0 {
                last_progress = Instant::now();
            } else if watchdog_ms > 0
                && last_progress.elapsed() >= Duration::from_millis(watchdog_ms)
            {
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
                // Keep actor handles alive for process lifetime in the current
                // scheduler model to avoid cross-thread teardown races.
            }
            unlink_pool(shard, pool);
            // Keep retired pool allocation alive to avoid concurrent reader UAFs on
            // raw pointers in the scheduler fast path. Handles are already decref'd
            // above, so leaking the container is acceptable for the current runtime
            // process model.
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
                current =
                    unsafe { (*prev).next_in_shard.load(Ordering::Acquire) as *const PoolHandle };
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
                if let Some(msg) = pool.queue.pop() {
                    let max_batch = pool.batch_limit;
                    let mut dispatched = 0i64;
                    let mut first = Some(msg);
                    while dispatched < max_batch {
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
                        dispatched += 1;
                        crate::actor::deliver_pool_message(msg);
                        inc_sched_dispatched();
                    }
                    dispatched_total += dispatched;
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
            let actor_handle = actor::actor_spawn(1, Value::nil(), 1, 3, 256, 10, 64);
            let handles = list::list_new(0);
            list::list_push(handles, actor_handle);
            let pool = actor::pool_new(handles, 0, 0, 0, 0, 256);
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
            let actor_handle = actor::actor_spawn(99, Value::nil(), 1, 3, 256, 10, 64);
            let handles = list::list_new(0);
            list::list_push(handles, actor_handle);
            let pool = actor::pool_new(handles, 0, 0, 0, 0, 256);
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
            let actor_handle = actor::actor_spawn(100, Value::nil(), 1, 3, 256, 10, 64);
            let handles = list::list_new(0);
            list::list_push(handles, actor_handle);
            let pool = actor::pool_new(handles, 0, 0, 0, 0, 256);

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
            let actor_handle = actor::actor_spawn(101, Value::nil(), 1, 3, 256, 10, 64);
            let handles = list::list_new(0);
            list::list_push(handles, actor_handle);
            let pool = actor::pool_new(handles, 0, 0, 0, 0, 256);

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
}
