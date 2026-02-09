pub(crate) mod actor {
    use crate::arena;
    use crate::config;
    #[cfg(feature = "metrics")]
    use crate::metrics::inc_alloc_pending;
    use crate::metrics::{
        inc_mailbox_enqueue_fail, inc_messages_dropped, inc_messages_dropped_paused,
        inc_pending_dropped, inc_pending_resolved, update_mailbox_high_water,
    };
    use crate::object::ObjHeader;
    use crate::reactor::task::TaskSignal;
    use crate::result;
    use crate::scheduler;
    use crate::value::{TypeId, Value, header};
    use crate::{wr_rc_dec, wr_rc_inc};
    use std::cell::UnsafeCell;
    use std::collections::{HashMap, VecDeque};
    use std::mem::MaybeUninit;
    #[cfg(test)]
    use std::sync::atomic::AtomicI8;
    use std::sync::atomic::{AtomicBool, AtomicI64, AtomicPtr, AtomicU64, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex, OnceLock};
    use std::time::{Duration, Instant};

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

    type MessageNodeHandle = *mut MessageNode;

    struct MessageNode {
        next: AtomicPtr<MessageNode>,
        msg: UnsafeCell<MaybeUninit<Message>>,
    }

    #[derive(Clone)]
    pub struct PoolMessage {
        mailbox: Arc<Mailbox>,
        node: MessageNodeHandle,
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
    unsafe impl Send for PoolSlot {}

    struct MailboxQueue {
        cap: usize,
        mask: usize,
        head: AtomicUsize,
        tail: AtomicUsize,
        slots: Box<[MailboxSlot]>,
    }

    struct MailboxSlot {
        seq: AtomicUsize,
        msg: UnsafeCell<MaybeUninit<MessageNodeHandle>>,
    }

    unsafe impl Sync for MailboxSlot {}
    unsafe impl Send for MailboxSlot {}
    unsafe impl Sync for MessageNode {}
    unsafe impl Send for MessageNode {}

    struct Mailbox {
        queue: MailboxQueue,
        work_notify: TaskSignal,
        space_notify: TaskSignal,
        space_notify_epoch: AtomicU64,
        space_notify_inflight: AtomicBool,
        len: AtomicUsize,
        closed: AtomicBool,
        paused: AtomicBool,
        pause_notify: TaskSignal,
        pause_epoch: AtomicUsize,
        pause_ack: AtomicUsize,
        pause_ack_notify: TaskSignal,
        enqueue_timeout: Duration,
        batch_limit: usize,
        arena: Mutex<arena::Arena>,
    }

    pub(crate) struct PendingState {
        lock: Mutex<Option<Value>>,
        notify: TaskSignal,
        dropped: AtomicBool,
    }

    struct MethodCache {
        class_id: u32,
        generation: u64,
        methods: HashMap<u32, MethodFn>,
        hot_method_id: u32,
        hot_method: Option<MethodFn>,
    }

    struct FastSendContext {
        mailbox: *const Mailbox,
        queue: VecDeque<MessageNodeHandle>,
        enqueued_since_flush: u64,
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum BurstTarget {
        Actor(*const ActorHandle),
        Pool(*const PoolHandle),
    }

    enum StagedMessage {
        Direct {
            mailbox: Arc<Mailbox>,
            node: MessageNodeHandle,
        },
        Pool {
            pool: *const PoolHandle,
            mailbox: Arc<Mailbox>,
            node: MessageNodeHandle,
        },
    }

    struct BurstContext {
        target: BurstTarget,
        depth: usize,
        staged: Vec<StagedMessage>,
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

    impl MailboxQueue {
        fn new(cap: usize) -> Self {
            let cap = cap.max(2).next_power_of_two();
            let mut slots = Vec::with_capacity(cap);
            for i in 0..cap {
                slots.push(MailboxSlot {
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

        fn push_node(&self, node: MessageNodeHandle) -> Result<(), MessageNodeHandle> {
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
                            (*slot.msg.get()).write(node);
                        }
                        slot.seq.store(tail + 1, Ordering::Release);
                        return Ok(());
                    }
                } else if diff < 0 {
                    return Err(node);
                }
                tail = self.tail.load(Ordering::Relaxed);
            }
        }

        fn pop_node(&self) -> Option<MessageNodeHandle> {
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
                        let node = unsafe { (*slot.msg.get()).assume_init_read() };
                        slot.seq.store(head + self.cap, Ordering::Release);
                        return Some(node);
                    }
                } else if diff < 0 {
                    return None;
                }
                head = self.head.load(Ordering::Relaxed);
            }
        }

        fn push_batch(&self, nodes: &[MessageNodeHandle]) -> usize {
            let mut accepted = 0usize;
            let mut stalls = 0usize;
            while accepted < nodes.len() {
                match self.push_node(nodes[accepted]) {
                    Ok(()) => {
                        accepted += 1;
                        stalls = 0;
                    }
                    Err(_) => {
                        stalls += 1;
                        if stalls >= MAILBOX_BATCH_ENQUEUE_SPIN_LIMIT {
                            break;
                        }
                        std::hint::spin_loop();
                    }
                }
            }
            accepted
        }

        fn push(&self, msg: Message) -> Result<(), Message> {
            let node = message_node_from_message(msg);
            match self.push_node(node) {
                Ok(()) => Ok(()),
                Err(node) => Err(message_node_into_message(node)),
            }
        }

        fn pop(&self) -> Option<Message> {
            self.pop_node().map(message_node_into_message)
        }
    }

    static METHODS: OnceLock<Mutex<HashMap<u32, HashMap<u32, MethodFn>>>> = OnceLock::new();
    static METHOD_REGISTRY_GENERATION: AtomicU64 = AtomicU64::new(1);
    static ARGS_POOL: OnceLock<Mutex<Vec<Vec<Value>>>> = OnceLock::new();
    static PENDING_POOL: OnceLock<Mutex<Vec<Arc<PendingState>>>> = OnceLock::new();
    static WATCHDOG_STARTED: OnceLock<()> = OnceLock::new();
    static FAST_PATH_ENV_ENABLED: OnceLock<bool> = OnceLock::new();
    static MESSAGE_NODE_GLOBAL_FREELIST: AtomicPtr<MessageNode> =
        AtomicPtr::new(std::ptr::null_mut());
    static BURST_ACTIVE_COUNT: AtomicUsize = AtomicUsize::new(0);
    #[cfg(test)]
    static FAST_PATH_TEST_OVERRIDE: AtomicI8 = AtomicI8::new(-1);
    const MESSAGE_NODE_FREELIST_BATCH: usize = 32;
    const BURST_STAGE_INITIAL_CAP: usize = 64;
    const BURST_MAX_STAGE: usize = 4096;
    const MAILBOX_BATCH_ENQUEUE_SPIN_LIMIT: usize = 2;
    const SPACE_NOTIFY_COALESCE_WINDOW: u64 = 1;

    thread_local! {
        static FAST_SEND_CONTEXT: UnsafeCell<Option<FastSendContext>> = const { UnsafeCell::new(None) };
        static BURST_CONTEXT: UnsafeCell<Option<BurstContext>> = const { UnsafeCell::new(None) };
        static MESSAGE_NODE_LOCAL_FREELIST: UnsafeCell<Vec<MessageNodeHandle>> = const { UnsafeCell::new(Vec::new()) };
    }

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

    fn method_registry_generation() -> u64 {
        METHOD_REGISTRY_GENERATION.load(Ordering::Acquire)
    }

    fn actor_fast_path_enabled() -> bool {
        #[cfg(test)]
        {
            match FAST_PATH_TEST_OVERRIDE.load(Ordering::Relaxed) {
                0 => return false,
                1 => return true,
                _ => {}
            }
        }
        *FAST_PATH_ENV_ENABLED
            .get_or_init(|| std::env::var_os("WRELA_ACTOR_FAST_PATH_DISABLE").is_none())
    }

    #[cfg(test)]
    fn set_actor_fast_path_for_test(enabled: Option<bool>) {
        let raw = match enabled {
            Some(true) => 1,
            Some(false) => 0,
            None => -1,
        };
        FAST_PATH_TEST_OVERRIDE.store(raw, Ordering::Relaxed);
    }

    fn snapshot_class_methods(class_id: u32) -> HashMap<u32, MethodFn> {
        let map = method_registry().lock().expect("method registry lock");
        map.get(&class_id).cloned().unwrap_or_default()
    }

    impl MethodCache {
        fn new(class_id: u32) -> Self {
            Self {
                class_id,
                generation: method_registry_generation(),
                methods: snapshot_class_methods(class_id),
                hot_method_id: u32::MAX,
                hot_method: None,
            }
        }

        fn resolve(&mut self, method_id: u32) -> Option<MethodFn> {
            if !actor_fast_path_enabled() {
                let map = method_registry().lock().expect("method registry lock");
                return map
                    .get(&self.class_id)
                    .and_then(|inner| inner.get(&method_id))
                    .copied();
            }
            let generation = method_registry_generation();
            if self.generation != generation {
                self.methods = snapshot_class_methods(self.class_id);
                self.generation = generation;
                self.hot_method_id = u32::MAX;
                self.hot_method = None;
            }
            if self.hot_method_id == method_id {
                return self.hot_method;
            }
            let method = self.methods.get(&method_id).copied();
            self.hot_method_id = method_id;
            self.hot_method = method;
            method
        }
    }

    fn fast_send_context_enter(mailbox: *const Mailbox) {
        FAST_SEND_CONTEXT.with(|slot| unsafe {
            *slot.get() = Some(FastSendContext {
                mailbox,
                queue: VecDeque::new(),
                enqueued_since_flush: 0,
            });
        });
    }

    fn fast_send_context_exit(mailbox: *const Mailbox) {
        FAST_SEND_CONTEXT.with(|slot| unsafe {
            let slot = &mut *slot.get();
            if let Some(ctx) = slot.as_ref() {
                if std::ptr::eq(ctx.mailbox, mailbox) {
                    *slot = None;
                }
            }
        });
    }

    fn fast_send_try_enqueue_node(
        mailbox: &Mailbox,
        node: MessageNodeHandle,
    ) -> Result<(), MessageNodeHandle> {
        FAST_SEND_CONTEXT.with(|slot| unsafe {
            let slot = &mut *slot.get();
            let Some(ctx) = slot.as_mut() else {
                return Err(node);
            };
            if !std::ptr::eq(ctx.mailbox, mailbox as *const Mailbox) {
                return Err(node);
            }
            // Self-send on the actor thread must never fall back to blocking channel send.
            // Channel fallback here can deadlock when the actor sends to itself while
            // currently processing a message.
            ctx.queue.push_back(node);
            ctx.enqueued_since_flush = ctx.enqueued_since_flush.saturating_add(1);
            Ok(())
        })
    }

    fn fast_send_try_dequeue_node(mailbox: &Mailbox) -> Option<MessageNodeHandle> {
        FAST_SEND_CONTEXT.with(|slot| unsafe {
            let slot = &mut *slot.get();
            let Some(ctx) = slot.as_mut() else {
                return None;
            };
            if !std::ptr::eq(ctx.mailbox, mailbox as *const Mailbox) {
                return None;
            }
            ctx.queue.pop_front()
        })
    }

    fn fast_send_take_enqueued_count(mailbox: &Mailbox) -> u64 {
        FAST_SEND_CONTEXT.with(|slot| unsafe {
            let slot = &mut *slot.get();
            let Some(ctx) = slot.as_mut() else {
                return 0;
            };
            if !std::ptr::eq(ctx.mailbox, mailbox as *const Mailbox) {
                return 0;
            }
            let count = ctx.enqueued_since_flush;
            ctx.enqueued_since_flush = 0;
            count
        })
    }

    fn message_node_alloc() -> MessageNodeHandle {
        Box::into_raw(Box::new(MessageNode {
            next: AtomicPtr::new(std::ptr::null_mut()),
            msg: UnsafeCell::new(MaybeUninit::uninit()),
        }))
    }

    fn message_node_global_pop() -> Option<MessageNodeHandle> {
        loop {
            let head = MESSAGE_NODE_GLOBAL_FREELIST.load(Ordering::Acquire);
            if head.is_null() {
                return None;
            }
            let next = unsafe { (*head).next.load(Ordering::Acquire) };
            if MESSAGE_NODE_GLOBAL_FREELIST
                .compare_exchange(head, next, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                return Some(head);
            }
        }
    }

    fn message_node_global_push(node: MessageNodeHandle) {
        loop {
            let head = MESSAGE_NODE_GLOBAL_FREELIST.load(Ordering::Acquire);
            unsafe {
                (*node).next.store(head, Ordering::Release);
            }
            if MESSAGE_NODE_GLOBAL_FREELIST
                .compare_exchange(head, node, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                return;
            }
        }
    }

    fn message_node_acquire() -> MessageNodeHandle {
        MESSAGE_NODE_LOCAL_FREELIST.with(|slot| unsafe {
            let local = &mut *slot.get();
            if let Some(node) = local.pop() {
                return node;
            }
            for _ in 0..MESSAGE_NODE_FREELIST_BATCH.saturating_sub(1) {
                if let Some(node) = message_node_global_pop() {
                    local.push(node);
                } else {
                    break;
                }
            }
            message_node_global_pop().unwrap_or_else(message_node_alloc)
        })
    }

    fn message_node_release(node: MessageNodeHandle) {
        MESSAGE_NODE_LOCAL_FREELIST.with(|slot| unsafe {
            let local = &mut *slot.get();
            local.push(node);
            if local.len() <= MESSAGE_NODE_FREELIST_BATCH * 16 {
                return;
            }
            for _ in 0..MESSAGE_NODE_FREELIST_BATCH {
                let Some(node) = local.pop() else { break };
                message_node_global_push(node);
            }
        });
    }

    fn message_node_from_message(msg: Message) -> MessageNodeHandle {
        let node = message_node_acquire();
        unsafe {
            (*(*node).msg.get()).write(msg);
        }
        node
    }

    fn message_node_into_message(node: MessageNodeHandle) -> Message {
        let msg = unsafe { (*(*node).msg.get()).assume_init_read() };
        message_node_release(node);
        msg
    }

    fn drop_message_node(node: MessageNodeHandle) {
        let msg = message_node_into_message(node);
        drop_message(msg);
    }

    fn process_message_node(
        mailbox: &Mailbox,
        method_cache: &mut MethodCache,
        node: MessageNodeHandle,
    ) {
        let msg = message_node_into_message(node);
        process_message(mailbox, method_cache, msg);
    }

    fn burst_target_from_handle(handle: Value) -> Option<BurstTarget> {
        if let Some(pool) = as_pool_ref(handle) {
            return Some(BurstTarget::Pool(pool));
        }
        as_actor(handle).map(BurstTarget::Actor)
    }

    fn burst_target_for_actor(actor: *const ActorHandle) -> BurstTarget {
        BurstTarget::Actor(actor)
    }

    fn burst_target_for_pool(pool: *const PoolHandle) -> BurstTarget {
        BurstTarget::Pool(pool)
    }

    fn burst_targets_match(left: BurstTarget, right: BurstTarget) -> bool {
        match (left, right) {
            (BurstTarget::Actor(a), BurstTarget::Actor(b)) => std::ptr::eq(a, b),
            (BurstTarget::Pool(a), BurstTarget::Pool(b)) => std::ptr::eq(a, b),
            _ => false,
        }
    }

    fn flush_staged_messages(target: BurstTarget, staged: Vec<StagedMessage>) {
        if staged.is_empty() {
            return;
        }
        match target {
            BurstTarget::Actor(_) => {
                let mut mailbox: Option<Arc<Mailbox>> = None;
                let mut nodes = Vec::with_capacity(staged.len());
                for item in staged {
                    if let StagedMessage::Direct {
                        mailbox: item_mailbox,
                        node,
                    } = item
                    {
                        if mailbox.is_none() {
                            mailbox = Some(item_mailbox);
                        }
                        nodes.push(node);
                    }
                }
                if let Some(mailbox) = mailbox {
                    enqueue_node_batch_ref(mailbox.as_ref(), &nodes);
                }
            }
            BurstTarget::Pool(_) => {
                let mut pool: Option<*const PoolHandle> = None;
                let mut batch = Vec::new();
                for item in staged {
                    if let StagedMessage::Pool {
                        pool: pool_ptr,
                        mailbox,
                        node,
                    } = item
                    {
                        if pool.is_none() {
                            pool = Some(pool_ptr);
                        }
                        batch.push(PoolMessage { mailbox, node });
                    }
                }
                if let Some(pool) = pool {
                    let accepted = scheduler::enqueue_batch(pool, &batch);
                    for msg in batch.into_iter().skip(accepted) {
                        drop_message_node(msg.node);
                        inc_messages_dropped();
                        inc_mailbox_enqueue_fail();
                    }
                }
            }
        }
    }

    fn burst_drop_staged(staged: Vec<StagedMessage>) {
        for item in staged {
            match item {
                StagedMessage::Direct { node, .. } => drop_message_node(node),
                StagedMessage::Pool { node, .. } => drop_message_node(node),
            }
        }
    }

    fn burst_stage_message(target: BurstTarget, msg: StagedMessage) -> bool {
        BURST_CONTEXT.with(|slot| unsafe {
            let slot = &mut *slot.get();
            let Some(ctx) = slot.as_mut() else {
                return false;
            };
            if !burst_targets_match(ctx.target, target) {
                return false;
            }
            if ctx.staged.len() >= BURST_MAX_STAGE {
                let staged =
                    std::mem::replace(&mut ctx.staged, Vec::with_capacity(BURST_STAGE_INITIAL_CAP));
                flush_staged_messages(ctx.target, staged);
            }
            ctx.staged.push(msg);
            true
        })
    }

    fn burst_is_active_for_target(target: BurstTarget) -> bool {
        if BURST_ACTIVE_COUNT.load(Ordering::Acquire) == 0 {
            return false;
        }
        BURST_CONTEXT.with(|slot| unsafe {
            let slot = &*slot.get();
            let Some(ctx) = slot.as_ref() else {
                return false;
            };
            burst_targets_match(ctx.target, target)
        })
    }

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
        METHOD_REGISTRY_GENERATION.fetch_add(1, Ordering::Release);
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
        let mailbox = Arc::new(Mailbox {
            queue: MailboxQueue::new(config.mailbox_cap),
            work_notify: TaskSignal::new(),
            space_notify: TaskSignal::new(),
            space_notify_epoch: AtomicU64::new(0),
            space_notify_inflight: AtomicBool::new(false),
            len: AtomicUsize::new(0),
            closed: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            pause_notify: TaskSignal::new(),
            pause_epoch: AtomicUsize::new(0),
            pause_ack: AtomicUsize::new(0),
            pause_ack_notify: TaskSignal::new(),
            enqueue_timeout: config.enqueue_timeout,
            batch_limit: config.batch_limit,
            arena: Mutex::new(arena::Arena::new(64 * 1024)),
        });
        unsafe {
            wr_rc_inc(instance);
        }
        let mailbox_for_loop = mailbox.clone();
        runtime_spawn(move || actor_loop(class_id, mailbox_for_loop));
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

    pub fn actor_fire_burst_begin(handle: Value) {
        let Some(target) = burst_target_from_handle(handle) else {
            runtime_error("fire_burst_begin: expected actor or pool handle");
            return;
        };
        BURST_CONTEXT.with(|slot| unsafe {
            let slot = &mut *slot.get();
            match slot.as_mut() {
                Some(ctx) => {
                    if burst_targets_match(ctx.target, target) {
                        ctx.depth = ctx.depth.saturating_add(1);
                    } else {
                        runtime_error("fire_burst_begin: active burst uses a different handle");
                    }
                }
                None => {
                    *slot = Some(BurstContext {
                        target,
                        depth: 1,
                        staged: Vec::with_capacity(BURST_STAGE_INITIAL_CAP),
                    });
                    BURST_ACTIVE_COUNT.fetch_add(1, Ordering::AcqRel);
                }
            }
        });
    }

    pub fn actor_fire_burst_end(handle: Value) {
        let Some(target) = burst_target_from_handle(handle) else {
            runtime_error("fire_burst_end: expected actor or pool handle");
            return;
        };
        let mut flush_target = None;
        let mut flush_staged = Vec::new();
        BURST_CONTEXT.with(|slot| unsafe {
            let slot = &mut *slot.get();
            let Some(ctx) = slot.as_mut() else {
                runtime_error("fire_burst_end: no active burst");
                return;
            };
            if !burst_targets_match(ctx.target, target) {
                runtime_error("fire_burst_end: handle does not match active burst");
                return;
            }
            if ctx.depth > 1 {
                ctx.depth -= 1;
                return;
            }
            flush_target = Some(ctx.target);
            flush_staged = std::mem::replace(&mut ctx.staged, Vec::new());
            *slot = None;
            BURST_ACTIVE_COUNT.fetch_sub(1, Ordering::AcqRel);
        });
        if let Some(target) = flush_target {
            flush_staged_messages(target, flush_staged);
        }
    }

    pub fn actor_fire_burst_abort(handle: Value) {
        let Some(target) = burst_target_from_handle(handle) else {
            runtime_error("fire_burst_abort: expected actor or pool handle");
            return;
        };
        let mut dropped = Vec::new();
        BURST_CONTEXT.with(|slot| unsafe {
            let slot = &mut *slot.get();
            let Some(ctx) = slot.as_mut() else {
                runtime_error("fire_burst_abort: no active burst");
                return;
            };
            if !burst_targets_match(ctx.target, target) {
                runtime_error("fire_burst_abort: handle does not match active burst");
                return;
            }
            dropped = std::mem::replace(&mut ctx.staged, Vec::new());
            *slot = None;
            BURST_ACTIVE_COUNT.fetch_sub(1, Ordering::AcqRel);
        });
        burst_drop_staged(dropped);
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
            enqueue_node_ref(&*(*actor).mailbox, msg);
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
        if burst_is_active_for_target(burst_target_for_actor(actor)) {
            let _ = burst_stage_message(
                burst_target_for_actor(actor),
                StagedMessage::Direct {
                    mailbox: unsafe { (*actor).mailbox.clone() },
                    node: msg,
                },
            );
        } else {
            unsafe {
                enqueue_node_ref(&*(*actor).mailbox, msg);
            }
        }
    }

    pub fn deliver_pool_message(msg: PoolMessage) {
        enqueue_node_ref(&msg.mailbox, msg.node);
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
            mailbox.work_notify.notify_waiters();
            mailbox
                .space_notify_inflight
                .store(false, Ordering::Release);
            mailbox.space_notify.notify_waiters();
            mailbox.pause_notify.notify_waiters();
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

    fn mailbox_dec_n(mailbox: &Mailbox, count: usize) {
        if count == 0 {
            return;
        }
        let prev = mailbox.len.fetch_sub(count, Ordering::AcqRel);
        if prev < count {
            mailbox.len.store(0, Ordering::Release);
            return;
        }
        if prev >= mailbox.queue.cap && prev - count < mailbox.queue.cap {
            maybe_notify_space(mailbox);
        }
        crate::metrics::inc_mailbox_dequeue_n(count as u64);
    }

    fn mailbox_dec(mailbox: &Mailbox) {
        mailbox_dec_n(mailbox, 1);
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
    ) -> Option<MessageNodeHandle> {
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
        Some(message_node_from_message(msg))
    }

    fn build_message_local(
        actor: *const ActorHandle,
        method_id: u32,
        argc: usize,
        argv_ptr: *const Value,
        pending: Option<Arc<PendingState>>,
    ) -> Option<MessageNodeHandle> {
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
        let node = build_message_inner(instance, method_id, args, pending)?;
        Some(PoolMessage { mailbox, node })
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
                enqueue_node_ref(&*(*actor).mailbox, msg);
                return Value::from_ptr(Box::into_raw(pending) as *mut ObjHeader);
            }

            let Some(msg) =
                build_pool_message(actor, method_id, argc, argv_ptr, Some(state.clone()))
            else {
                recycle_pending_state(state);
                return Value::nil();
            };

            if let Err(msg) = scheduler::enqueue(pool, msg) {
                drop_message_node(msg.node);
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
                if burst_is_active_for_target(burst_target_for_pool(pool)) {
                    let _ = burst_stage_message(
                        burst_target_for_pool(pool),
                        StagedMessage::Direct {
                            mailbox: (*actor).mailbox.clone(),
                            node: msg,
                        },
                    );
                    return;
                }
                enqueue_node_ref(&*(*actor).mailbox, msg);
                return;
            }

            if let Some(msg) = build_pool_message(actor, method_id, argc, argv_ptr, None) {
                if burst_is_active_for_target(burst_target_for_pool(pool)) {
                    let _ = burst_stage_message(
                        burst_target_for_pool(pool),
                        StagedMessage::Pool {
                            pool,
                            mailbox: msg.mailbox.clone(),
                            node: msg.node,
                        },
                    );
                    return;
                }
                if let Err(msg) = scheduler::enqueue(pool, msg) {
                    drop_message_node(msg.node);
                }
            }
        }
    }

    fn maybe_notify_space(mailbox: &Mailbox) {
        let _ = mailbox
            .space_notify_epoch
            .fetch_add(SPACE_NOTIFY_COALESCE_WINDOW, Ordering::AcqRel)
            + 1;
        if !mailbox.space_notify_inflight.swap(true, Ordering::AcqRel) {
            mailbox.space_notify.notify_one();
        }
    }

    fn enqueue_message(mailbox: Arc<Mailbox>, msg: Message) {
        enqueue_message_ref(&mailbox, msg);
    }

    fn enqueue_messages_ref(mailbox: &Mailbox, msgs: Vec<Message>) {
        let nodes = msgs
            .into_iter()
            .map(message_node_from_message)
            .collect::<Vec<_>>();
        enqueue_node_batch_ref(mailbox, &nodes);
    }

    fn enqueue_message_ref(mailbox: &Mailbox, msg: Message) {
        enqueue_node_ref(mailbox, message_node_from_message(msg));
    }

    fn enqueue_node_ref(mailbox: &Mailbox, node: MessageNodeHandle) {
        let one = [node];
        enqueue_node_batch_ref(mailbox, &one);
    }

    fn enqueue_node_batch_ref(mailbox: &Mailbox, nodes: &[MessageNodeHandle]) {
        if nodes.is_empty() {
            return;
        }
        if mailbox.closed.load(Ordering::Acquire) {
            for node in nodes.iter().copied() {
                drop_message_node(node);
                inc_mailbox_enqueue_fail();
                inc_messages_dropped();
            }
            return;
        }
        if nodes.len() == 1 {
            let node = nodes[0];
            if fast_send_try_enqueue_node(mailbox, node).is_ok() {
                return;
            }
        }
        let deadline = Instant::now() + mailbox.enqueue_timeout;
        let mut idx = 0usize;
        loop {
            let accepted = mailbox.queue.push_batch(&nodes[idx..]);
            if accepted > 0 {
                let prev = mailbox.len.fetch_add(accepted, Ordering::AcqRel);
                let len = prev + accepted;
                update_mailbox_high_water(len);
                crate::metrics::inc_mailbox_enqueue_ok_n(accepted as u64);
                crate::metrics::inc_messages_sent_n(accepted as u64);
                idx += accepted;
                if prev == 0 {
                    mailbox.work_notify.notify_one();
                }
                if idx >= nodes.len() {
                    mailbox
                        .space_notify_inflight
                        .store(false, Ordering::Release);
                    return;
                }
            } else {
                if mailbox.closed.load(Ordering::Acquire) {
                    for node in nodes[idx..].iter().copied() {
                        drop_message_node(node);
                        inc_mailbox_enqueue_fail();
                        inc_messages_dropped();
                    }
                    return;
                }
                let now = Instant::now();
                if now >= deadline {
                    for node in nodes[idx..].iter().copied() {
                        drop_message_node(node);
                        inc_mailbox_enqueue_fail();
                        inc_messages_dropped();
                    }
                    return;
                }
                let observed = mailbox.space_notify.snapshot();
                let remaining = deadline.saturating_duration_since(now);
                let wait_for = remaining.min(Duration::from_micros(250));
                let _ = mailbox.space_notify.wait_timeout(observed, wait_for);
                mailbox
                    .space_notify_inflight
                    .store(false, Ordering::Release);
            }
        }
    }

    fn actor_loop(class_id: u32, mailbox: Arc<Mailbox>) {
        let mailbox_ptr = Arc::as_ptr(&mailbox);
        let fast_path = actor_fast_path_enabled();
        if fast_path {
            fast_send_context_enter(mailbox_ptr);
        }
        let mut method_cache = MethodCache::new(class_id);
        loop {
            let (msg, counted) = match mailbox_recv(&mailbox, fast_path) {
                Some(pair) => pair,
                None => break,
            };
            while mailbox.paused.load(Ordering::Acquire) {
                let epoch = mailbox.pause_epoch.load(Ordering::Acquire);
                mailbox.pause_ack.store(epoch, Ordering::Release);
                mailbox.pause_ack_notify.notify_waiters();
                let _ = mailbox.pause_notify.wait(mailbox.pause_notify.snapshot());
            }
            if handle_message(&mailbox, msg, counted, &mut method_cache) {
                std::thread::yield_now();
            } else {
                break;
            }
            if fast_path {
                let enqueued = fast_send_take_enqueued_count(&mailbox);
                if enqueued > 0 {
                    crate::metrics::inc_mailbox_enqueue_ok_n(enqueued);
                    crate::metrics::inc_messages_sent_n(enqueued);
                }
            }
        }
        if fast_path {
            let enqueued = fast_send_take_enqueued_count(&mailbox);
            if enqueued > 0 {
                crate::metrics::inc_mailbox_enqueue_ok_n(enqueued);
                crate::metrics::inc_messages_sent_n(enqueued);
            }
            fast_send_context_exit(mailbox_ptr);
        }
    }

    fn mailbox_recv(mailbox: &Mailbox, fast_path: bool) -> Option<(MessageNodeHandle, bool)> {
        loop {
            if fast_path {
                if let Some(msg) = fast_send_try_dequeue_node(mailbox) {
                    return Some((msg, false));
                }
            }
            if let Some(msg) = mailbox.queue.pop_node() {
                return Some((msg, true));
            }
            if mailbox.closed.load(Ordering::Acquire) && mailbox.len.load(Ordering::Acquire) == 0 {
                return None;
            }
            let observed = mailbox.work_notify.snapshot();
            if fast_path {
                if let Some(msg) = fast_send_try_dequeue_node(mailbox) {
                    return Some((msg, false));
                }
            }
            if let Some(msg) = mailbox.queue.pop_node() {
                return Some((msg, true));
            }
            if mailbox.closed.load(Ordering::Acquire) && mailbox.len.load(Ordering::Acquire) == 0 {
                return None;
            }
            let _ = mailbox.work_notify.wait(observed);
        }
    }

    fn handle_message(
        mailbox: &Mailbox,
        first: MessageNodeHandle,
        first_counted: bool,
        method_cache: &mut MethodCache,
    ) -> bool {
        let batch_limit = mailbox.batch_limit;
        let mut current = first;
        let mut current_counted = first_counted;
        let mut processed = 0usize;
        for idx in 0..batch_limit {
            if mailbox.closed.load(Ordering::Acquire) {
                drop_message_node(current);
                if current_counted {
                    mailbox_dec_n(mailbox, processed + 1);
                } else {
                    mailbox_dec_n(mailbox, processed);
                }
                while let Some(msg) = fast_send_try_dequeue_node(mailbox) {
                    drop_message_node(msg);
                }
                drain_messages(mailbox);
                return false;
            }
            process_message_node(mailbox, method_cache, current);
            if current_counted {
                processed += 1;
            }
            if idx + 1 >= batch_limit {
                break;
            }
            if let Some(next) = fast_send_try_dequeue_node(mailbox) {
                current = next;
                current_counted = false;
                continue;
            }
            match mailbox.queue.pop_node() {
                Some(next) => {
                    current = next;
                    current_counted = true;
                }
                None => break,
            }
        }
        mailbox_dec_n(mailbox, processed);
        true
    }

    fn drain_messages(mailbox: &Mailbox) {
        loop {
            let msg = match mailbox.queue.pop_node() {
                Some(msg) => msg,
                None => break,
            };
            drop_message_node(msg);
            mailbox_dec(mailbox);
        }
    }

    fn process_message(mailbox: &Mailbox, method_cache: &mut MethodCache, msg: Message) {
        let mut arena_guard = mailbox.arena.lock().expect("arena lock");
        let _guard = arena::enter(&mut *arena_guard as *mut _);
        let func = method_cache.resolve(msg.method_id);
        if func.is_none() && crate::config::debug_actor_enabled() {
            eprintln!(
                "actor: missing method class_id={} method_id={} argc={}",
                method_cache.class_id,
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
        use crate::value::int_value;
        use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicUsize, Ordering};
        use std::sync::{Arc, Mutex, OnceLock};
        use std::thread;
        use std::time::{Duration, Instant};

        fn metrics_test_lock() -> &'static Mutex<()> {
            static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
            LOCK.get_or_init(|| Mutex::new(()))
        }

        fn burst_test_lock() -> &'static Mutex<()> {
            static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
            LOCK.get_or_init(|| Mutex::new(()))
        }

        fn dummy_mailbox() -> Arc<Mailbox> {
            Arc::new(Mailbox {
                queue: MailboxQueue::new(1),
                work_notify: TaskSignal::new(),
                space_notify: TaskSignal::new(),
                space_notify_epoch: AtomicU64::new(0),
                space_notify_inflight: AtomicBool::new(false),
                len: AtomicUsize::new(0),
                closed: AtomicBool::new(false),
                paused: AtomicBool::new(false),
                pause_notify: TaskSignal::new(),
                pause_epoch: AtomicUsize::new(0),
                pause_ack: AtomicUsize::new(0),
                pause_ack_notify: TaskSignal::new(),
                enqueue_timeout: Duration::from_millis(1),
                batch_limit: 1,
                arena: Mutex::new(arena::Arena::new(1024)),
            })
        }

        fn test_mailbox() -> Arc<Mailbox> {
            let mailbox = Arc::new(Mailbox {
                queue: MailboxQueue::new(1),
                work_notify: TaskSignal::new(),
                space_notify: TaskSignal::new(),
                space_notify_epoch: AtomicU64::new(0),
                space_notify_inflight: AtomicBool::new(false),
                len: AtomicUsize::new(0),
                closed: AtomicBool::new(false),
                paused: AtomicBool::new(false),
                pause_notify: TaskSignal::new(),
                pause_epoch: AtomicUsize::new(0),
                pause_ack: AtomicUsize::new(0),
                pause_ack_notify: TaskSignal::new(),
                enqueue_timeout: Duration::from_millis(1),
                batch_limit: 1,
                arena: Mutex::new(arena::Arena::new(1024)),
            });
            mailbox
        }

        fn ordering_log() -> &'static Mutex<Vec<i64>> {
            static LOG: OnceLock<Mutex<Vec<i64>>> = OnceLock::new();
            LOG.get_or_init(|| Mutex::new(Vec::new()))
        }

        fn burst_ordering_log() -> &'static Mutex<Vec<i64>> {
            static LOG: OnceLock<Mutex<Vec<i64>>> = OnceLock::new();
            LOG.get_or_init(|| Mutex::new(Vec::new()))
        }

        fn at_most_once_counts() -> &'static Mutex<HashMap<i64, usize>> {
            static COUNTS: OnceLock<Mutex<HashMap<i64, usize>>> = OnceLock::new();
            COUNTS.get_or_init(|| Mutex::new(HashMap::new()))
        }

        fn throughput_counter() -> &'static AtomicUsize {
            static COUNTER: OnceLock<AtomicUsize> = OnceLock::new();
            COUNTER.get_or_init(|| AtomicUsize::new(0))
        }

        fn throughput_target() -> &'static AtomicUsize {
            static TARGET: OnceLock<AtomicUsize> = OnceLock::new();
            TARGET.get_or_init(|| AtomicUsize::new(0))
        }

        fn throughput_fanout() -> &'static AtomicUsize {
            static FANOUT: OnceLock<AtomicUsize> = OnceLock::new();
            FANOUT.get_or_init(|| AtomicUsize::new(1))
        }

        fn throughput_actor_bits() -> &'static AtomicU64 {
            static HANDLE_BITS: OnceLock<AtomicU64> = OnceLock::new();
            HANDLE_BITS.get_or_init(|| AtomicU64::new(Value::nil().0))
        }

        extern "C" fn record_ordering(argc: usize, argv: *const Value) -> Value {
            if argc > 1 && !argv.is_null() {
                let args = unsafe { std::slice::from_raw_parts(argv, argc) };
                if let Some(tag) = int_value(args[1]) {
                    ordering_log().lock().expect("ordering log lock").push(tag);
                }
            }
            Value::nil()
        }

        extern "C" fn record_burst_ordering(argc: usize, argv: *const Value) -> Value {
            if argc > 1 && !argv.is_null() {
                let args = unsafe { std::slice::from_raw_parts(argv, argc) };
                if let Some(tag) = int_value(args[1]) {
                    burst_ordering_log()
                        .lock()
                        .expect("burst ordering log lock")
                        .push(tag);
                }
            }
            Value::nil()
        }

        extern "C" fn record_at_most_once(argc: usize, argv: *const Value) -> Value {
            if argc > 1 && !argv.is_null() {
                let args = unsafe { std::slice::from_raw_parts(argv, argc) };
                if let Some(tag) = int_value(args[1]) {
                    let mut counts = at_most_once_counts().lock().expect("counts lock");
                    let entry = counts.entry(tag).or_insert(0);
                    *entry += 1;
                }
            }
            Value::nil()
        }

        extern "C" fn throughput_ping_fanout(_argc: usize, _argv: *const Value) -> Value {
            let completed = throughput_counter().fetch_add(1, Ordering::Relaxed) + 1;
            let total = throughput_target().load(Ordering::Acquire);
            if completed >= total {
                return Value::nil();
            }
            let fanout = throughput_fanout().load(Ordering::Relaxed).max(1);
            let remaining = total - completed;
            let sends = fanout.min(remaining);
            let actor = Value(throughput_actor_bits().load(Ordering::Acquire));
            for _ in 0..sends {
                actor_fire(actor, 0, 0, std::ptr::null());
            }
            Value::nil()
        }

        fn wait_until<F>(timeout: Duration, mut predicate: F) -> bool
        where
            F: FnMut() -> bool,
        {
            let deadline = Instant::now() + timeout;
            while Instant::now() < deadline {
                if predicate() {
                    return true;
                }
                thread::sleep(Duration::from_millis(1));
            }
            predicate()
        }

        struct FastPathOverrideGuard;

        impl Drop for FastPathOverrideGuard {
            fn drop(&mut self) {
                set_actor_fast_path_for_test(None);
            }
        }

        #[cfg(feature = "metrics")]
        #[test]
        fn mailbox_metrics_enqueue_dequeue() {
            let _guard = metrics_test_lock().lock().expect("metrics test lock");
            metrics::reset();
            let mailbox = test_mailbox();

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

            let received = mailbox.queue.pop().expect("recv message");
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
            let mailbox = test_mailbox();
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
        fn actor_fire_burst_flush_order_preserved() {
            const CLASS_ID: u32 = 36_001;
            let _guard = burst_test_lock().lock().expect("burst test lock");
            burst_ordering_log()
                .lock()
                .expect("burst ordering log lock")
                .clear();
            register_method(CLASS_ID, 0, record_burst_ordering);
            let actor = actor_spawn(CLASS_ID as u64, Value::nil(), 1, 3, 256, 10, 64);

            actor_fire_burst_begin(actor);
            for tag in [1i64, 2, 3] {
                let args = [Value::from_int(tag)];
                actor_fire(actor, 0, 1, args.as_ptr());
            }
            assert_eq!(
                burst_ordering_log()
                    .lock()
                    .expect("burst ordering log lock")
                    .len(),
                0,
                "messages should remain staged before burst end"
            );
            actor_fire_burst_end(actor);
            assert!(
                wait_until(Duration::from_secs(5), || {
                    burst_ordering_log()
                        .lock()
                        .expect("burst ordering log lock")
                        .len()
                        == 3
                }),
                "timed out waiting for staged burst flush"
            );
            let log = burst_ordering_log()
                .lock()
                .expect("burst ordering log lock")
                .clone();
            assert_eq!(log, vec![1, 2, 3]);
            unsafe {
                wr_rc_dec(actor);
            }
        }

        #[test]
        fn actor_fire_burst_nested_same_handle_ok() {
            const CLASS_ID: u32 = 36_002;
            let _guard = burst_test_lock().lock().expect("burst test lock");
            burst_ordering_log()
                .lock()
                .expect("burst ordering log lock")
                .clear();
            register_method(CLASS_ID, 0, record_burst_ordering);
            let actor = actor_spawn(CLASS_ID as u64, Value::nil(), 1, 3, 256, 10, 64);

            actor_fire_burst_begin(actor);
            actor_fire_burst_begin(actor);
            let args = [Value::from_int(7)];
            actor_fire(actor, 0, 1, args.as_ptr());
            actor_fire_burst_end(actor);
            thread::sleep(Duration::from_millis(10));
            assert_eq!(
                burst_ordering_log()
                    .lock()
                    .expect("burst ordering log lock")
                    .len(),
                0,
                "nested end should not flush until outer end"
            );
            actor_fire_burst_end(actor);
            assert!(
                wait_until(Duration::from_secs(5), || {
                    burst_ordering_log()
                        .lock()
                        .expect("burst ordering log lock")
                        .len()
                        == 1
                }),
                "timed out waiting for nested burst flush"
            );
            let log = burst_ordering_log()
                .lock()
                .expect("burst ordering log lock")
                .clone();
            assert_eq!(log, vec![7]);
            unsafe {
                wr_rc_dec(actor);
            }
        }

        #[test]
        fn actor_fire_burst_mismatched_handle_rejected() {
            const CLASS_ID: u32 = 36_003;
            let _guard = burst_test_lock().lock().expect("burst test lock");
            burst_ordering_log()
                .lock()
                .expect("burst ordering log lock")
                .clear();
            register_method(CLASS_ID, 0, record_burst_ordering);
            let actor_a = actor_spawn(CLASS_ID as u64, Value::nil(), 1, 3, 256, 10, 64);
            let actor_b = actor_spawn(CLASS_ID as u64, Value::nil(), 1, 3, 256, 10, 64);

            actor_fire_burst_begin(actor_a);
            let args = [Value::from_int(11)];
            actor_fire(actor_a, 0, 1, args.as_ptr());
            actor_fire_burst_begin(actor_b);
            actor_fire_burst_end(actor_b);
            thread::sleep(Duration::from_millis(10));
            assert_eq!(
                burst_ordering_log()
                    .lock()
                    .expect("burst ordering log lock")
                    .len(),
                0,
                "mismatched handle calls should not flush active burst"
            );
            actor_fire_burst_end(actor_a);
            assert!(
                wait_until(Duration::from_secs(5), || {
                    burst_ordering_log()
                        .lock()
                        .expect("burst ordering log lock")
                        .len()
                        == 1
                }),
                "timed out waiting for burst flush"
            );
            let log = burst_ordering_log()
                .lock()
                .expect("burst ordering log lock")
                .clone();
            assert_eq!(log, vec![11]);
            unsafe {
                wr_rc_dec(actor_a);
                wr_rc_dec(actor_b);
            }
        }

        #[test]
        fn actor_fire_burst_abort_reclaims_nodes() {
            const CLASS_ID: u32 = 36_004;
            let _guard = burst_test_lock().lock().expect("burst test lock");
            burst_ordering_log()
                .lock()
                .expect("burst ordering log lock")
                .clear();
            register_method(CLASS_ID, 0, record_burst_ordering);
            let actor = actor_spawn(CLASS_ID as u64, Value::nil(), 1, 3, 256, 10, 64);

            actor_fire_burst_begin(actor);
            for tag in [21i64, 22, 23] {
                let args = [Value::from_int(tag)];
                actor_fire(actor, 0, 1, args.as_ptr());
            }
            actor_fire_burst_abort(actor);
            thread::sleep(Duration::from_millis(20));
            assert_eq!(
                burst_ordering_log()
                    .lock()
                    .expect("burst ordering log lock")
                    .len(),
                0,
                "aborted staged messages should not be delivered"
            );
            unsafe {
                wr_rc_dec(actor);
            }
        }

        #[test]
        fn message_slab_reuse_smoke() {
            let mailbox = test_mailbox();
            for _ in 0..128 {
                let msg = Message {
                    method_id: 0,
                    args: take_args_vec(),
                    pending: None,
                };
                enqueue_message(mailbox.clone(), msg);
                if let Some(received) = mailbox.queue.pop() {
                    drop_message(received);
                    mailbox_dec(mailbox.as_ref());
                }
            }
            let pooled = args_pool().lock().expect("args pool lock").len();
            assert!(
                pooled > 0,
                "message argument pool did not retain reusable entries"
            );
        }

        #[test]
        fn message_slab_no_leak_on_enqueue_fail() {
            let mailbox = test_mailbox();
            mailbox.closed.store(true, Ordering::Release);
            for _ in 0..128 {
                let msg = Message {
                    method_id: 0,
                    args: take_args_vec(),
                    pending: None,
                };
                enqueue_message(mailbox.clone(), msg);
            }
            let pooled = args_pool().lock().expect("args pool lock").len();
            assert!(
                pooled > 0,
                "enqueue-fail path should recycle message storage"
            );
        }

        #[test]
        fn mailbox_push_batch_preserves_order() {
            let queue = MailboxQueue::new(8);
            let mut nodes = Vec::new();
            for tag in [1i64, 2, 3, 4] {
                let mut args = take_args_vec();
                args.push(Value::from_int(tag));
                nodes.push(message_node_from_message(Message {
                    method_id: 7,
                    args,
                    pending: None,
                }));
            }
            let accepted = queue.push_batch(&nodes);
            assert_eq!(accepted, nodes.len());
            let mut out = Vec::new();
            while let Some(node) = queue.pop_node() {
                let msg = message_node_into_message(node);
                out.push(crate::value::int_value(msg.args[0]).unwrap_or_default());
                drop_message(msg);
            }
            assert_eq!(out, vec![1, 2, 3, 4]);
        }

        #[test]
        fn mailbox_push_batch_partial_full_returns_count() {
            let queue = MailboxQueue::new(2);
            let mut nodes = Vec::new();
            for tag in [10i64, 11, 12] {
                let mut args = take_args_vec();
                args.push(Value::from_int(tag));
                nodes.push(message_node_from_message(Message {
                    method_id: 9,
                    args,
                    pending: None,
                }));
            }
            let accepted = queue.push_batch(&nodes);
            assert!(accepted <= 2);
            for node in nodes.into_iter().skip(accepted) {
                drop_message_node(node);
            }
            while let Some(node) = queue.pop_node() {
                drop_message_node(node);
            }
        }

        #[test]
        fn burst_flush_uses_single_wake_edge() {
            let mailbox = Arc::new(Mailbox {
                queue: MailboxQueue::new(8),
                work_notify: TaskSignal::new(),
                space_notify: TaskSignal::new(),
                space_notify_epoch: AtomicU64::new(0),
                space_notify_inflight: AtomicBool::new(false),
                len: AtomicUsize::new(0),
                closed: AtomicBool::new(false),
                paused: AtomicBool::new(false),
                pause_notify: TaskSignal::new(),
                pause_epoch: AtomicUsize::new(0),
                pause_ack: AtomicUsize::new(0),
                pause_ack_notify: TaskSignal::new(),
                enqueue_timeout: Duration::from_millis(1),
                batch_limit: 1,
                arena: Mutex::new(arena::Arena::new(1024)),
            });
            let mut nodes = Vec::new();
            for _ in 0..4 {
                nodes.push(message_node_from_message(Message {
                    method_id: 0,
                    args: take_args_vec(),
                    pending: None,
                }));
            }
            let before = mailbox.work_notify.snapshot();
            enqueue_node_batch_ref(mailbox.as_ref(), &nodes);
            let after = mailbox.work_notify.snapshot();
            assert_eq!(
                after,
                before + 1,
                "batch flush should notify work edge once"
            );
            while let Some(node) = mailbox.queue.pop_node() {
                drop_message_node(node);
                mailbox_dec(mailbox.as_ref());
            }
        }

        fn dummy_pool_handle_for_batch(queue_cap: usize) -> Box<PoolHandle> {
            Box::new(PoolHandle {
                header: header(TypeId::Pool),
                pool_id: 0,
                shard_id: 0,
                objective: 0,
                pool_size: 0,
                rr: AtomicUsize::new(0),
                handles: Vec::new(),
                queue: PoolQueue::new(queue_cap),
                credits: AtomicI64::new(0),
                min_share: 0,
                max_share: 0,
                weight: 0,
                has_ready: AtomicBool::new(false),
                alive: AtomicBool::new(true),
                enqueue_inflight: AtomicUsize::new(0),
                next_in_shard: AtomicUsize::new(0),
                batch_limit: 1,
                drop_on_full: false,
                shard_hint: AtomicUsize::new(0),
            })
        }

        #[test]
        fn pool_enqueue_batch_preserves_order() {
            let pool = dummy_pool_handle_for_batch(8);
            let pool_ptr = &*pool as *const PoolHandle;
            let mailbox = dummy_mailbox();
            let mut batch = Vec::new();
            for tag in [31i64, 32, 33] {
                let mut args = take_args_vec();
                args.push(Value::from_int(tag));
                batch.push(PoolMessage {
                    mailbox: mailbox.clone(),
                    node: message_node_from_message(Message {
                        method_id: 1,
                        args,
                        pending: None,
                    }),
                });
            }
            let accepted = crate::scheduler::enqueue_batch(pool_ptr, &batch);
            assert_eq!(accepted, batch.len());
            let mut out = Vec::new();
            while let Some(msg) = unsafe { (*pool_ptr).queue.pop() } {
                let inner = message_node_into_message(msg.node);
                out.push(crate::value::int_value(inner.args[0]).unwrap_or_default());
                drop_message(inner);
            }
            assert_eq!(out, vec![31, 32, 33]);
        }

        #[test]
        fn pool_enqueue_batch_partial_full_drops_tail_safely() {
            let pool = dummy_pool_handle_for_batch(2);
            let pool_ptr = &*pool as *const PoolHandle;
            let mailbox = dummy_mailbox();
            let mut batch = Vec::new();
            for tag in [41i64, 42, 43] {
                let mut args = take_args_vec();
                args.push(Value::from_int(tag));
                batch.push(PoolMessage {
                    mailbox: mailbox.clone(),
                    node: message_node_from_message(Message {
                        method_id: 2,
                        args,
                        pending: None,
                    }),
                });
            }
            let accepted = crate::scheduler::enqueue_batch(pool_ptr, &batch);
            assert!(accepted <= 2);
            for msg in batch.into_iter().skip(accepted) {
                drop_message_node(msg.node);
            }
            while let Some(msg) = unsafe { (*pool_ptr).queue.pop() } {
                drop_message_node(msg.node);
            }
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
                            node: message_node_from_message(Message {
                                method_id: 0,
                                args: Vec::new(),
                                pending: None,
                            }),
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
                if let Some(msg) = queue.pop() {
                    drop_message_node(msg.node);
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
                            node: message_node_from_message(Message {
                                method_id: 0,
                                args: Vec::new(),
                                pending: None,
                            }),
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
                if let Some(msg) = queue.pop() {
                    drop_message_node(msg.node);
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
                node: message_node_from_message(Message {
                    method_id: 0,
                    args: Vec::new(),
                    pending: None,
                }),
            };
            let _ = crate::scheduler::enqueue(pool_ptr, msg);
            assert!(metrics::get(metrics::METRIC_POOL_ENQUEUE_AFTER_RETIRE) >= 1);
            unsafe {
                wr_rc_dec(pool);
                wr_rc_dec(handles);
                wr_rc_dec(actor_handle);
            }
        }

        #[test]
        fn actor_concurrent_send_preserves_per_sender_order() {
            const CLASS_ID: u32 = 32_001;
            const PRODUCERS: usize = 6;
            const PER_PRODUCER: usize = 300;
            let total = PRODUCERS * PER_PRODUCER;
            ordering_log().lock().expect("ordering log lock").clear();
            register_method(CLASS_ID, 0, record_ordering);
            let actor = actor_spawn(CLASS_ID as u64, Value::nil(), 1, 3, 256, 10, 64);

            let mut producers = Vec::new();
            for producer in 0..PRODUCERS {
                producers.push(thread::spawn(move || {
                    for seq in 0..PER_PRODUCER {
                        let tag = ((producer as i64) << 32) | seq as i64;
                        let args = [Value::from_int(tag)];
                        actor_fire(actor, 0, 1, args.as_ptr());
                    }
                }));
            }
            for producer in producers {
                producer.join().expect("producer join");
            }

            assert!(
                wait_until(Duration::from_secs(10), || {
                    ordering_log().lock().expect("ordering log lock").len() == total
                }),
                "timed out waiting for all ordered messages"
            );
            let log = ordering_log().lock().expect("ordering log lock").clone();
            assert_eq!(log.len(), total);

            let mut next = vec![0usize; PRODUCERS];
            for tag in log {
                let producer = (tag >> 32) as usize;
                let seq = (tag as u64 & 0xffff_ffff) as usize;
                assert!(producer < PRODUCERS, "unexpected producer id in tag {tag}");
                assert_eq!(
                    seq, next[producer],
                    "producer {producer} sequence out of order"
                );
                next[producer] += 1;
            }
            for observed in next {
                assert_eq!(observed, PER_PRODUCER);
            }

            unsafe { wr_rc_dec(actor) };
        }

        #[test]
        fn actor_concurrent_send_is_at_most_once() {
            const CLASS_ID: u32 = 32_002;
            const PRODUCERS: usize = 8;
            const PER_PRODUCER: usize = 250;
            let total = PRODUCERS * PER_PRODUCER;
            at_most_once_counts().lock().expect("counts lock").clear();
            register_method(CLASS_ID, 0, record_at_most_once);
            let actor = actor_spawn(CLASS_ID as u64, Value::nil(), 1, 3, 256, 10, 64);

            let mut producers = Vec::new();
            for producer in 0..PRODUCERS {
                producers.push(thread::spawn(move || {
                    for seq in 0..PER_PRODUCER {
                        let tag = (producer * PER_PRODUCER + seq) as i64;
                        let args = [Value::from_int(tag)];
                        actor_fire(actor, 0, 1, args.as_ptr());
                    }
                }));
            }
            for producer in producers {
                producer.join().expect("producer join");
            }

            assert!(
                wait_until(Duration::from_secs(10), || {
                    let counts = at_most_once_counts().lock().expect("counts lock");
                    counts.len() == total && counts.values().all(|count| *count == 1)
                }),
                "timed out waiting for at-most-once delivery"
            );
            let counts = at_most_once_counts().lock().expect("counts lock");
            assert_eq!(counts.len(), total);
            assert!(counts.values().all(|count| *count == 1));

            unsafe { wr_rc_dec(actor) };
        }

        fn run_actor_throughput_lane(class_id: u32, fast_path: bool) -> f64 {
            const PRODUCERS: usize = 1;
            const SEED_MESSAGES_PER_PRODUCER: usize = 1;
            const TOTAL_MESSAGES: usize = 20_000;
            const FANOUT: usize = 1;
            set_actor_fast_path_for_test(Some(fast_path));
            throughput_counter().store(0, Ordering::Relaxed);
            throughput_target().store(TOTAL_MESSAGES, Ordering::Relaxed);
            throughput_fanout().store(FANOUT, Ordering::Relaxed);
            register_method(class_id, 0, throughput_ping_fanout);
            let actor = actor_spawn(class_id as u64, Value::nil(), 1, 3, 1024, 10, 128);
            throughput_actor_bits().store(actor.0, Ordering::Release);

            let start = Instant::now();
            let mut producers = Vec::new();
            for _ in 0..PRODUCERS {
                producers.push(thread::spawn(move || {
                    for _ in 0..SEED_MESSAGES_PER_PRODUCER {
                        actor_fire(actor, 0, 0, std::ptr::null());
                    }
                }));
            }
            for producer in producers {
                producer.join().expect("producer join");
            }
            let completed_ok = wait_until(Duration::from_secs(30), || {
                throughput_counter().load(Ordering::Acquire) >= TOTAL_MESSAGES
            });
            assert!(
                completed_ok,
                "timed out waiting for throughput lane completion: fast_path={} completed={} target={}",
                fast_path,
                throughput_counter().load(Ordering::Acquire),
                TOTAL_MESSAGES
            );
            let elapsed = start.elapsed();
            throughput_actor_bits().store(Value::nil().0, Ordering::Release);
            throughput_target().store(0, Ordering::Relaxed);
            unsafe { wr_rc_dec(actor) };
            TOTAL_MESSAGES as f64 / elapsed.as_secs_f64()
        }

        #[test]
        #[ignore]
        fn actor_fast_path_throughput_artifact() {
            let _override_guard = FastPathOverrideGuard;
            let baseline = run_actor_throughput_lane(32_101, false);
            let fast_path = run_actor_throughput_lane(32_102, true);
            let improvement_pct = if baseline > 0.0 {
                (fast_path - baseline) / baseline * 100.0
            } else {
                0.0
            };
            let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
            let repo_root = manifest_dir.parent().unwrap_or(manifest_dir);
            let artifact_dir = repo_root.join(".artifacts/wre-412");
            std::fs::create_dir_all(&artifact_dir).expect("create artifact dir");
            let artifact = artifact_dir.join("actor_throughput.txt");
            let body = format!(
                "benchmark=ping_pong_fanout_single_actor\nproducers=1\nseed_messages_per_producer=1\nfanout=1\ntotal_messages=20000\nbaseline_msgs_per_sec={baseline:.2}\nfast_path_msgs_per_sec={fast_path:.2}\nimprovement_pct={improvement_pct:.2}\n"
            );
            std::fs::write(&artifact, body).expect("write throughput artifact");
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
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::OnceLock;
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
    pub const METRIC_ABI_TYPED_LANE: u32 = 21;
    pub const METRIC_ABI_BOXED_LANE: u32 = 22;

    const METRIC_COUNT: usize = 64;
    static METRICS: [AtomicU64; METRIC_COUNT] = [const { AtomicU64::new(0) }; METRIC_COUNT];
    static MAILBOX_HIGH_WATER: AtomicU64 = AtomicU64::new(0);
    static METRICS_DUMP_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

    #[inline(always)]
    fn bump(id: u32) {
        if let Some(metric) = METRICS.get(id as usize) {
            metric.fetch_add(1, Ordering::Relaxed);
            maybe_dump_metrics();
        }
    }

    #[inline(always)]
    fn bump_by(id: u32, value: u64) {
        if value == 0 {
            return;
        }
        if let Some(metric) = METRICS.get(id as usize) {
            metric.fetch_add(value, Ordering::Relaxed);
            maybe_dump_metrics();
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
        maybe_dump_metrics();
    }

    pub fn install_dump_hook() {
        maybe_dump_metrics();
    }

    fn metrics_dump_path() -> Option<PathBuf> {
        METRICS_DUMP_PATH
            .get_or_init(|| env::var("WRELA_METRICS_PATH").ok().map(PathBuf::from))
            .clone()
    }

    fn maybe_dump_metrics() {
        let Some(path) = metrics_dump_path() else {
            return;
        };
        let data = format!(
            "{{\"messages_sent\":{},\"messages_dropped\":{},\"pending_resolved\":{},\"pending_dropped\":{},\"mailbox_high_water\":{},\"rc_inc\":{},\"rc_dec\":{},\"alloc_list\":{},\"alloc_map\":{},\"alloc_string\":{},\"alloc_bytes\":{},\"alloc_result\":{},\"alloc_pending\":{},\"mailbox_enqueue_ok\":{},\"mailbox_enqueue_fail\":{},\"mailbox_dequeue\":{},\"sched_dispatched\":{},\"sched_skipped_no_credit\":{},\"abi_typed_lane\":{},\"abi_boxed_lane\":{}}}",
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
            get(METRIC_ABI_TYPED_LANE),
            get(METRIC_ABI_BOXED_LANE),
        );
        let _ = fs::write(path, data.as_bytes());
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
    pub fn inc_abi_typed_lane() {
        bump(METRIC_ABI_TYPED_LANE)
    }
    pub fn inc_abi_boxed_lane() {
        bump(METRIC_ABI_BOXED_LANE)
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
        maybe_dump_metrics();
    }
}

pub(crate) mod scheduler {
    use crate::actor::{PoolHandle, PoolMessage, deliver_pool_message};
    use crate::config::{sched_ready_cap, sched_shards, sched_tick_ms};
    use crate::diagnostics;
    use crate::metrics::{
        inc_pool_enqueue_after_retire, inc_pool_queue_full, inc_sched_dispatched,
        inc_sched_skipped_no_credit,
    };
    use crate::reactor::task::TaskSignal;
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex, OnceLock};
    use std::time::{Duration, Instant};

    const OBJECTIVE_COUNT: usize = 4;
    #[cfg(test)]
    const OBJECTIVE_THROUGHPUT: usize = 1;
    #[cfg(test)]
    const OBJECTIVE_BALANCE: usize = 3;
    const STARVATION_BOUND_TICKS: usize = 12;

    #[derive(Clone, Copy)]
    struct DispatchProfile {
        quantum: i64,
        batch_cap: i64,
    }

    const DISPATCH_PROFILES: [DispatchProfile; OBJECTIVE_COUNT] = [
        DispatchProfile {
            quantum: 6,
            batch_cap: 8,
        },
        DispatchProfile {
            quantum: 10,
            batch_cap: 64,
        },
        DispatchProfile {
            quantum: 2,
            batch_cap: 4,
        },
        DispatchProfile {
            quantum: 4,
            batch_cap: 16,
        },
    ];

    #[derive(Clone)]
    struct ObjectiveDispatchState {
        deficits: [i64; OBJECTIVE_COUNT],
        wait_ticks: [usize; OBJECTIVE_COUNT],
        cursor: usize,
        starvation_bound_ticks: usize,
    }

    impl ObjectiveDispatchState {
        fn new() -> Self {
            Self {
                deficits: [0; OBJECTIVE_COUNT],
                wait_ticks: [0; OBJECTIVE_COUNT],
                cursor: 0,
                starvation_bound_ticks: STARVATION_BOUND_TICKS,
            }
        }

        fn select_objective(&mut self, ready: [bool; OBJECTIVE_COUNT]) -> Option<usize> {
            if !ready.iter().any(|is_ready| *is_ready) {
                self.deficits = [0; OBJECTIVE_COUNT];
                self.wait_ticks = [0; OBJECTIVE_COUNT];
                return None;
            }

            for idx in 0..OBJECTIVE_COUNT {
                if ready[idx] {
                    self.wait_ticks[idx] = self.wait_ticks[idx].saturating_add(1);
                } else {
                    self.wait_ticks[idx] = 0;
                    self.deficits[idx] = 0;
                }
            }

            let mut forced = None;
            let mut longest_wait = 0usize;
            for idx in 0..OBJECTIVE_COUNT {
                if ready[idx]
                    && self.wait_ticks[idx] >= self.starvation_bound_ticks
                    && self.wait_ticks[idx] >= longest_wait
                {
                    longest_wait = self.wait_ticks[idx];
                    forced = Some(idx);
                }
            }

            let selected = forced.or_else(|| self.select_with_deficit(ready));
            if let Some(idx) = selected {
                self.cursor = (idx + 1) % OBJECTIVE_COUNT;
                self.wait_ticks[idx] = 0;
                self.deficits[idx] = self.deficits[idx].saturating_sub(1);
            }
            selected
        }

        fn select_with_deficit(&mut self, ready: [bool; OBJECTIVE_COUNT]) -> Option<usize> {
            for _ in 0..2 {
                for step in 0..OBJECTIVE_COUNT {
                    let idx = (self.cursor + step) % OBJECTIVE_COUNT;
                    if ready[idx] && self.deficits[idx] > 0 {
                        return Some(idx);
                    }
                }
                for idx in 0..OBJECTIVE_COUNT {
                    if ready[idx] {
                        let quantum = DISPATCH_PROFILES[idx].quantum.max(1);
                        self.deficits[idx] = self.deficits[idx].saturating_add(quantum);
                    }
                }
            }
            None
        }
    }

    static SHARDS: OnceLock<Vec<Arc<SchedulerShard>>> = OnceLock::new();
    static POOL_COUNTER: AtomicU64 = AtomicU64::new(1);

    pub struct SchedulerShard {
        ready_by_objective: [ReadyQueue; OBJECTIVE_COUNT],
        head: AtomicUsize,
        notify: TaskSignal,
        has_work: AtomicBool,
        retired: Mutex<Vec<usize>>,
    }

    impl SchedulerShard {
        fn new(_id: usize) -> Self {
            Self {
                ready_by_objective: std::array::from_fn(|_| ReadyQueue::new(sched_ready_cap())),
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
            let ready = shard.has_ready_work();
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
                    let objective = objective_index((*pool).objective);
                    let _ = shard.ready_by_objective[objective].push(pool as usize);
                }
                if !shard.has_work.swap(true, Ordering::AcqRel) {
                    shard.notify.notify_one();
                }
            }
            Ok(())
        }
    }

    pub fn enqueue_batch(pool: *const PoolHandle, msgs: &[PoolMessage]) -> usize {
        unsafe {
            if pool.is_null() || msgs.is_empty() {
                return 0;
            }
            (*pool).enqueue_inflight.fetch_add(1, Ordering::AcqRel);
            if !(*pool).alive.load(Ordering::Acquire) || (*pool).drop_on_full {
                if !(*pool).alive.load(Ordering::Acquire) {
                    inc_pool_enqueue_after_retire();
                } else {
                    inc_pool_queue_full();
                }
                (*pool).enqueue_inflight.fetch_sub(1, Ordering::AcqRel);
                return 0;
            }
            let queue = &(*pool).queue;
            let mut accepted = 0usize;
            for msg in msgs {
                if queue.push(msg.clone()).is_ok() {
                    accepted += 1;
                } else {
                    inc_pool_queue_full();
                    break;
                }
            }
            (*pool).enqueue_inflight.fetch_sub(1, Ordering::AcqRel);
            if accepted > 0 {
                let shard_id = (*pool).shard_id as usize;
                if let Some(shard) = SHARDS.get().and_then(|s| s.get(shard_id)) {
                    if !(*pool).has_ready.swap(true, Ordering::AcqRel) {
                        let objective = objective_index((*pool).objective);
                        let _ = shard.ready_by_objective[objective].push(pool as usize);
                    }
                    if !shard.has_work.swap(true, Ordering::AcqRel) {
                        shard.notify.notify_one();
                    }
                }
            }
            accepted
        }
    }

    fn scheduler_loop(shard: Arc<SchedulerShard>) {
        let tick = Duration::from_millis(sched_tick_ms());
        let mut dispatch_state = ObjectiveDispatchState::new();
        let mut last_progress = Instant::now();
        let watchdog_ms = crate::config::sched_watchdog_ms();
        loop {
            if !shard.has_work.load(Ordering::Acquire) {
                let observed_epoch = shard.notify.snapshot();
                let _ = shard.notify.wait_timeout(observed_epoch, tick);
            }
            let dispatched = dispatch_ready(&shard, &mut dispatch_state);
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
            if !shard.has_ready_work() {
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
            if other.has_ready_work() {
                if let Some((objective, pool_ptr)) = other.pop_ready_any() {
                    let _ = shard.ready_by_objective[objective].push(pool_ptr);
                    shard.has_work.store(true, Ordering::Release);
                    break;
                }
            }
        }
    }

    fn dispatch_ready(shard: &SchedulerShard, dispatch_state: &mut ObjectiveDispatchState) -> i64 {
        let mut dispatched_total = 0i64;
        loop {
            let ready = shard.ready_mask();
            let Some(objective) = dispatch_state.select_objective(ready) else {
                break;
            };
            let pool_ptr = shard.ready_by_objective[objective].pop();
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
                    let profile = DISPATCH_PROFILES[objective];
                    let max_batch = pool.batch_limit.min(profile.batch_cap).max(1);
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
                        let requeue_objective = objective_index(pool.objective);
                        let _ = shard.ready_by_objective[requeue_objective].push(pool_ptr as usize);
                    } else {
                        pool.has_ready.store(false, Ordering::Release);
                    }
                } else {
                    inc_sched_skipped_no_credit();
                    pool.has_ready.store(false, Ordering::Release);
                }
            }
        }
        dispatched_total
    }

    fn objective_index(objective: u8) -> usize {
        usize::from(objective).min(OBJECTIVE_COUNT - 1)
    }

    impl SchedulerShard {
        fn ready_mask(&self) -> [bool; OBJECTIVE_COUNT] {
            std::array::from_fn(|idx| self.ready_by_objective[idx].peek_has_data())
        }

        fn has_ready_work(&self) -> bool {
            self.ready_mask().iter().any(|has_data| *has_data)
        }

        fn pop_ready_any(&self) -> Option<(usize, usize)> {
            for objective in 0..OBJECTIVE_COUNT {
                if let Some(pool_ptr) = self.ready_by_objective[objective].pop() {
                    return Some((objective, pool_ptr));
                }
            }
            None
        }
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
        use std::time::{Duration, Instant};

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
        fn objective_dispatch_policy_bounds_starvation() {
            let mut state = ObjectiveDispatchState::new();
            let ready = [true, true, true, true];
            let mut current_gap = [0usize; OBJECTIVE_COUNT];
            let mut max_gap = [0usize; OBJECTIVE_COUNT];
            let rounds = 20_000usize;

            for _ in 0..rounds {
                let objective = state
                    .select_objective(ready)
                    .expect("all objectives are ready");
                for idx in 0..OBJECTIVE_COUNT {
                    if idx == objective {
                        current_gap[idx] = 0;
                    } else {
                        current_gap[idx] = current_gap[idx].saturating_add(1);
                        max_gap[idx] = max_gap[idx].max(current_gap[idx]);
                    }
                }
            }

            for gap in max_gap {
                assert!(
                    gap <= STARVATION_BOUND_TICKS,
                    "observed starvation gap {gap} exceeds bound {}",
                    STARVATION_BOUND_TICKS
                );
            }
        }

        fn run_pool_throughput_lane(class_id: u32, objective: i64) -> f64 {
            const TOTAL_MESSAGES: usize = 30_000;
            const FANOUT: usize = 2;
            static POOL_COUNTER: OnceLock<AtomicUsize> = OnceLock::new();
            static POOL_TARGET: OnceLock<AtomicUsize> = OnceLock::new();
            static POOL_FANOUT: OnceLock<AtomicUsize> = OnceLock::new();
            static POOL_BITS: OnceLock<std::sync::atomic::AtomicU64> = OnceLock::new();

            extern "C" fn throughput_ping_pool(_argc: usize, _argv: *const Value) -> Value {
                let counter = POOL_COUNTER.get().expect("pool counter");
                let target = POOL_TARGET.get().expect("pool target");
                let fanout = POOL_FANOUT.get().expect("pool fanout");
                let pool_bits = POOL_BITS.get().expect("pool bits");

                let completed = counter.fetch_add(1, Ordering::Relaxed) + 1;
                let total = target.load(Ordering::Acquire);
                if completed >= total {
                    return Value::nil();
                }
                let sends = fanout.load(Ordering::Acquire).max(1).min(total - completed);
                let pool = Value(pool_bits.load(Ordering::Acquire));
                for _ in 0..sends {
                    actor::actor_fire(pool, 0, 0, std::ptr::null());
                }
                Value::nil()
            }

            POOL_COUNTER
                .get_or_init(|| AtomicUsize::new(0))
                .store(0, Ordering::Relaxed);
            POOL_TARGET
                .get_or_init(|| AtomicUsize::new(0))
                .store(TOTAL_MESSAGES, Ordering::Relaxed);
            POOL_FANOUT
                .get_or_init(|| AtomicUsize::new(1))
                .store(FANOUT, Ordering::Relaxed);
            actor::register_method(class_id, 0, throughput_ping_pool);

            let actor_handle_a =
                actor::actor_spawn(class_id as u64, Value::nil(), 1, 3, 1024, 10, 128);
            let actor_handle_b =
                actor::actor_spawn(class_id as u64, Value::nil(), 1, 3, 1024, 10, 128);
            let handles = list::list_new(0);
            list::list_push(handles, actor_handle_a);
            list::list_push(handles, actor_handle_b);
            let pool = actor::pool_new(handles, objective, 0, 0, 1, 4096);
            POOL_BITS
                .get_or_init(|| std::sync::atomic::AtomicU64::new(Value::nil().0))
                .store(pool.0, Ordering::Release);

            let start = Instant::now();
            actor::actor_fire(pool, 0, 0, std::ptr::null());
            let completed_ok = {
                let deadline = Instant::now() + Duration::from_secs(30);
                loop {
                    if Instant::now() >= deadline {
                        break false;
                    }
                    if POOL_COUNTER
                        .get()
                        .expect("pool counter")
                        .load(Ordering::Acquire)
                        >= TOTAL_MESSAGES
                    {
                        break true;
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
            };
            assert!(
                completed_ok,
                "timed out waiting for throughput lane objective={} completed={} target={}",
                objective,
                POOL_COUNTER
                    .get()
                    .expect("pool counter")
                    .load(Ordering::Acquire),
                TOTAL_MESSAGES
            );
            let elapsed = start.elapsed();
            POOL_BITS
                .get_or_init(|| std::sync::atomic::AtomicU64::new(Value::nil().0))
                .store(Value::nil().0, Ordering::Release);
            unsafe {
                wr_rc_dec(pool);
                wr_rc_dec(handles);
                wr_rc_dec(actor_handle_a);
                wr_rc_dec(actor_handle_b);
            }
            TOTAL_MESSAGES as f64 / elapsed.as_secs_f64()
        }

        #[test]
        #[ignore]
        fn objective_policy_throughput_artifact() {
            let compile_heavy_baseline = run_pool_throughput_lane(41_401, OBJECTIVE_BALANCE as i64);
            let compile_heavy_matched =
                run_pool_throughput_lane(41_402, OBJECTIVE_THROUGHPUT as i64);
            let improvement_pct = if compile_heavy_baseline > 0.0 {
                (compile_heavy_matched - compile_heavy_baseline) / compile_heavy_baseline * 100.0
            } else {
                0.0
            };
            assert!(
                improvement_pct >= 20.0,
                "expected >=20% throughput improvement, saw {improvement_pct:.2}% (baseline={compile_heavy_baseline:.2}, matched={compile_heavy_matched:.2})"
            );

            let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
            let repo_root = manifest_dir.parent().unwrap_or(manifest_dir);
            let artifact_dir = repo_root.join(".artifacts/wre-414");
            std::fs::create_dir_all(&artifact_dir).expect("create artifact dir");
            let artifact = artifact_dir.join("scheduler_objective_throughput.txt");
            let body = format!(
                "benchmark=compile_heavy_pool_lane\nbaseline_objective=balance\nmatched_objective=throughput\ntotal_messages=30000\nfanout=2\nbaseline_msgs_per_sec={compile_heavy_baseline:.2}\nmatched_msgs_per_sec={compile_heavy_matched:.2}\nimprovement_pct={improvement_pct:.2}\nstarvation_bound_ticks={STARVATION_BOUND_TICKS}\nprofiles=latency(quantum=6,batch=8),throughput(quantum=10,batch=64),conservation(quantum=2,batch=4),balance(quantum=4,batch=16)\n"
            );
            std::fs::write(&artifact, body).expect("write throughput artifact");
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
