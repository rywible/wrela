use crate::object::ObjHeader;
use crate::value::{header, TypeId, Value};
use crate::result;
use crate::{wr_rc_dec, wr_rc_inc};
use crate::metrics::{inc_messages_dropped, inc_messages_sent, inc_pending_dropped, inc_pending_resolved, update_mailbox_high_water};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread;

pub type MethodFn = extern "C" fn(argc: usize, argv: *const Value) -> Value;

#[repr(C)]
pub struct ActorHandle {
    header: ObjHeader,
    class_id: u32,
    mailbox: Arc<Mailbox>,
    instance: Value,
}

#[repr(C)]
pub struct PendingObj {
    header: ObjHeader,
    state: Arc<PendingState>,
}

struct PendingState {
    lock: Mutex<Option<Value>>,
    cv: Condvar,
    dropped: AtomicBool,
}

struct Message {
    method_id: u32,
    args: Vec<Value>,
    pending: Option<Arc<PendingState>>,
}

struct Mailbox {
    lock: Mutex<MailboxState>,
    scheduled: AtomicBool,
    class_id: u32,
}

struct MailboxState {
    queue: RingBuffer,
    closed: bool,
    capacity: usize,
}

static METHODS: OnceLock<Mutex<HashMap<u32, HashMap<u32, MethodFn>>>> = OnceLock::new();
static ARGS_POOL: OnceLock<Mutex<Vec<Vec<Value>>>> = OnceLock::new();
static SCHEDULER: OnceLock<Scheduler> = OnceLock::new();

fn method_registry() -> &'static Mutex<HashMap<u32, HashMap<u32, MethodFn>>> {
    METHODS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn args_pool() -> &'static Mutex<Vec<Vec<Value>>> {
    ARGS_POOL.get_or_init(|| Mutex::new(Vec::new()))
}

const ARGS_POOL_MAX_CAP: usize = 64;
const ARGS_POOL_MAX_LEN: usize = 128;
const WORKER_BATCH_SIZE: usize = 32;

struct RingBuffer {
    buf: Vec<Option<Message>>,
    head: usize,
    tail: usize,
    len: usize,
}

impl RingBuffer {
    fn with_capacity(cap: usize) -> Self {
        let cap = cap.max(1);
        let mut buf = Vec::with_capacity(cap);
        buf.resize_with(cap, || None);
        RingBuffer {
            buf,
            head: 0,
            tail: 0,
            len: 0,
        }
    }

    fn len(&self) -> usize {
        self.len
    }

    fn capacity(&self) -> usize {
        self.buf.len()
    }

    fn push(&mut self, msg: Message) -> Result<(), Message> {
        if self.len == self.capacity() {
            return Err(msg);
        }
        self.buf[self.tail] = Some(msg);
        self.tail = (self.tail + 1) % self.capacity();
        self.len += 1;
        Ok(())
    }

    fn pop(&mut self) -> Option<Message> {
        if self.len == 0 {
            return None;
        }
        let msg = self.buf[self.head].take();
        self.head = (self.head + 1) % self.capacity();
        self.len -= 1;
        msg
    }

    fn drain_all(&mut self) -> Vec<Message> {
        let mut out = Vec::with_capacity(self.len);
        while let Some(msg) = self.pop() {
            out.push(msg);
        }
        out
    }
}

struct Scheduler {
    inner: Arc<SchedulerInner>,
    _workers: Vec<thread::JoinHandle<()>>,
}

struct SchedulerInner {
    wait_lock: Mutex<()>,
    cv: Condvar,
    workers: Vec<WorkerQueue>,
    rr: std::sync::atomic::AtomicUsize,
}

struct WorkerQueue {
    queue: Mutex<VecDeque<Arc<Mailbox>>>,
}

impl Scheduler {
    fn new() -> Self {
        let worker_count = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        let mut workers_vec = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            workers_vec.push(WorkerQueue {
                queue: Mutex::new(VecDeque::new()),
            });
        }
        let inner = Arc::new(SchedulerInner {
            wait_lock: Mutex::new(()),
            cv: Condvar::new(),
            workers: workers_vec,
            rr: std::sync::atomic::AtomicUsize::new(0),
        });
        let mut workers = Vec::with_capacity(worker_count);
        for id in 0..worker_count {
            let inner_ref = inner.clone();
            workers.push(thread::spawn(move || {
                worker_loop(id, inner_ref);
            }));
        }
        Scheduler {
            inner,
            _workers: workers,
        }
    }

    fn schedule(&self, mailbox: Arc<Mailbox>) {
        let idx = self
            .inner
            .rr
            .fetch_add(1, Ordering::Relaxed)
            % self.inner.workers.len();
        let worker = &self.inner.workers[idx];
        let mut guard = worker.queue.lock().expect("worker queue lock");
        guard.push_back(mailbox);
        self.inner.cv.notify_one();
    }

    fn schedule_to(&self, mailbox: Arc<Mailbox>, worker_id: usize) {
        let idx = worker_id % self.inner.workers.len();
        let worker = &self.inner.workers[idx];
        let mut guard = worker.queue.lock().expect("worker queue lock");
        guard.push_back(mailbox);
        self.inner.cv.notify_one();
    }
}

fn scheduler() -> &'static Scheduler {
    SCHEDULER.get_or_init(Scheduler::new)
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

pub fn register_method(class_id: u32, method_id: u32, func: MethodFn) {
    let mut map = method_registry().lock().expect("method registry lock");
    map.entry(class_id)
        .or_insert_with(HashMap::new)
        .insert(method_id, func);
}

pub fn actor_spawn(class_id: u32, instance: Value) -> Value {
    let capacity = 1024;
    let mailbox = Arc::new(Mailbox {
        lock: Mutex::new(MailboxState {
            queue: RingBuffer::with_capacity(capacity),
            closed: false,
            capacity,
        }),
        scheduled: AtomicBool::new(false),
        class_id,
    });
    unsafe {
        wr_rc_inc(instance);
    }
    let obj = Box::new(ActorHandle {
        header: header(TypeId::Actor),
        class_id,
        mailbox,
        instance,
    });
    Value::from_ptr(Box::into_raw(obj) as *mut ObjHeader)
}

pub fn actor_send(handle: Value, method_id: u32, argc: usize, argv_ptr: *const Value) -> Value {
    let actor = match as_actor(handle) {
        Some(actor) => actor,
        None => return Value::nil(),
    };
    let mailbox = unsafe { (*actor).mailbox.clone() };
    let instance = unsafe { (*actor).instance };
    let args = if argc == 0 {
        &[]
    } else if argv_ptr.is_null() {
        return Value::nil();
    } else {
        unsafe { std::slice::from_raw_parts(argv_ptr, argc) }
    };
    let mut args_vec = take_args_vec();
    args_vec.push(instance);
    if !args.is_empty() {
        args_vec.extend_from_slice(args);
    }
    let state = Arc::new(PendingState {
        lock: Mutex::new(None),
        cv: Condvar::new(),
        dropped: AtomicBool::new(false),
    });
    let pending = Box::new(PendingObj {
        header: header(TypeId::Pending),
        state: state.clone(),
    });
    let msg = Message {
        method_id,
        args: args_vec,
        pending: Some(state),
    };
    unsafe {
        for arg in &msg.args {
            wr_rc_inc(*arg);
        }
    }
    enqueue_message(mailbox, msg);
    Value::from_ptr(Box::into_raw(pending) as *mut ObjHeader)
}

pub fn actor_fire(handle: Value, method_id: u32, argc: usize, argv_ptr: *const Value) {
    let actor = match as_actor(handle) {
        Some(actor) => actor,
        None => return,
    };
    let mailbox = unsafe { (*actor).mailbox.clone() };
    let instance = unsafe { (*actor).instance };
    let args = if argc == 0 {
        &[]
    } else if argv_ptr.is_null() {
        return;
    } else {
        unsafe { std::slice::from_raw_parts(argv_ptr, argc) }
    };
    let mut args_vec = take_args_vec();
    args_vec.push(instance);
    if !args.is_empty() {
        args_vec.extend_from_slice(args);
    }
    let msg = Message {
        method_id,
        args: args_vec,
        pending: None,
    };
    unsafe {
        for arg in &msg.args {
            wr_rc_inc(*arg);
        }
    }
    enqueue_message(mailbox, msg);
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
        let mut guard = p.state.lock.lock().expect("pending lock");
        loop {
            if let Some(val) = *guard {
                return result::result_ok(val);
            }
            guard = p.state.cv.wait(guard).expect("pending wait");
        }
    }
}

pub unsafe fn drop_actor(ptr: *mut ObjHeader) {
    let actor = ptr as *mut ActorHandle;
    unsafe {
        let mailbox = (*actor).mailbox.clone();
        let instance = (*actor).instance;
        let drained = {
            let mut state = mailbox.lock.lock().expect("mailbox lock");
            state.closed = true;
            state.queue.drain_all()
        };
        mailbox.scheduled.store(false, Ordering::Release);
        wr_rc_dec(instance);
        drop(Box::from_raw(actor));
        for msg in drained {
            if let Some(pending) = msg.pending {
                resolve_pending(&pending, Value::nil());
            }
            for arg in msg.args {
                wr_rc_dec(arg);
            }
        }
    }
}

pub unsafe fn drop_pending(ptr: *mut ObjHeader) {
    let pending = ptr as *mut PendingObj;
    unsafe {
        let state = (*pending).state.clone();
        state.dropped.store(true, Ordering::Release);
        inc_pending_dropped();
        {
            let mut guard = state.lock.lock().expect("pending lock");
            if let Some(val) = guard.take() {
                wr_rc_dec(val);
            }
        }
        drop(Box::from_raw(pending));
    }
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

fn enqueue_message(mailbox: Arc<Mailbox>, msg: Message) {
    let mut state = mailbox.lock.lock().expect("mailbox lock");
    if state.closed {
        if let Some(pending) = msg.pending {
            resolve_pending(&pending, Value::nil());
        }
        let args = msg.args;
        for arg in args.iter().copied() {
            unsafe { wr_rc_dec(arg) };
        }
        return_args_vec(args);
        inc_messages_dropped();
        return;
    }
    if state.queue.len() >= state.capacity {
        if let Some(pending) = msg.pending {
            resolve_pending(&pending, Value::nil());
        }
        let args = msg.args;
        for arg in args.iter().copied() {
            unsafe { wr_rc_dec(arg) };
        }
        return_args_vec(args);
        inc_messages_dropped();
        return;
    }
    if state.queue.push(msg).is_err() {
        inc_messages_dropped();
        return;
    }
    inc_messages_sent();
    update_mailbox_high_water(state.queue.len());
    let should_schedule = !mailbox.scheduled.swap(true, Ordering::AcqRel);
    drop(state);
    if should_schedule {
        scheduler().schedule(mailbox);
    }
}

fn worker_loop(id: usize, inner: Arc<SchedulerInner>) {
    loop {
        if let Some(mailbox) = pop_local_or_steal(id, &inner) {
            process_mailbox(mailbox, id);
            continue;
        }
        let guard = inner.wait_lock.lock().expect("scheduler wait lock");
        drop(inner.cv.wait(guard).expect("scheduler wait"));
    }
}

fn pop_local_or_steal(id: usize, inner: &Arc<SchedulerInner>) -> Option<Arc<Mailbox>> {
    {
        let worker = &inner.workers[id];
        let mut guard = worker.queue.lock().expect("worker queue lock");
        if let Some(mailbox) = guard.pop_front() {
            return Some(mailbox);
        }
    }
    let worker_count = inner.workers.len();
    for offset in 1..worker_count {
        let victim = (id + offset) % worker_count;
        let worker = &inner.workers[victim];
        let mut guard = worker.queue.lock().expect("worker queue lock");
        if let Some(mailbox) = guard.pop_back() {
            return Some(mailbox);
        }
    }
    None
}

fn process_mailbox(mailbox: Arc<Mailbox>, worker_id: usize) {
    for _ in 0..WORKER_BATCH_SIZE {
        let msg = {
            let mut state = mailbox.lock.lock().expect("mailbox lock");
            if state.closed {
                let drained = state.queue.drain_all();
                drop(state);
                drain_messages(drained);
                mailbox.scheduled.store(false, Ordering::Release);
                return;
            }
            state.queue.pop()
        };
        let msg = match msg {
            Some(msg) => msg,
            None => break,
        };
        process_message(mailbox.class_id, msg);
    }

    let mut state = mailbox.lock.lock().expect("mailbox lock");
    if state.closed {
        let drained = state.queue.drain_all();
        drop(state);
        drain_messages(drained);
        mailbox.scheduled.store(false, Ordering::Release);
        return;
    }
    if state.queue.len() == 0 {
        mailbox.scheduled.store(false, Ordering::Release);
        return;
    }
    drop(state);
    scheduler().schedule_to(mailbox, worker_id);
}

fn process_message(class_id: u32, msg: Message) {
    let func = {
        let map = method_registry().lock().expect("method registry lock");
        map.get(&class_id)
            .and_then(|inner| inner.get(&msg.method_id))
            .copied()
    };
    if func.is_none() && std::env::var("WR_DEBUG_ACTOR").is_ok() {
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
    if std::env::var("WR_DEBUG_ACTOR").is_ok() {
        eprintln!("actor: method result raw={}", result.0);
    }
    if let Some(pending) = msg.pending {
        resolve_pending(&pending, result);
    }
    unsafe {
        for arg in msg.args.iter().copied() {
            wr_rc_dec(arg);
        }
    }
    return_args_vec(msg.args);
}

fn drain_messages(messages: Vec<Message>) {
    for msg in messages {
        if let Some(pending) = msg.pending {
            resolve_pending(&pending, Value::nil());
        }
        unsafe {
            for arg in msg.args.iter().copied() {
                wr_rc_dec(arg);
            }
        }
        return_args_vec(msg.args);
    }
}

fn resolve_pending(pending: &Arc<PendingState>, value: Value) {
    if pending.dropped.load(Ordering::Acquire) {
        unsafe { wr_rc_dec(value) };
        return;
    }
    let mut guard = pending.lock.lock().expect("pending lock");
    if pending.dropped.load(Ordering::Acquire) {
        drop(guard);
        unsafe { wr_rc_dec(value) };
        return;
    }
    *guard = Some(value);
    inc_pending_resolved();
    pending.cv.notify_all();
}
