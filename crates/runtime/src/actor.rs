use crate::config::{actor_config_for_objective, normalize_objective, normalize_pool_size};
use crate::metrics::{
    inc_messages_dropped, inc_messages_dropped_paused, inc_messages_sent, inc_pending_dropped,
    inc_pending_resolved, update_mailbox_high_water,
};
use crate::object::ObjHeader;
use crate::result;
use crate::value::{header, TypeId, Value};
use crate::{wr_rc_dec, wr_rc_inc};
use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::runtime::{Handle, Runtime};
use tokio::sync::mpsc::error::{SendTimeoutError, TryRecvError};
use tokio::sync::{mpsc, Notify};

pub type MethodFn = extern "C" fn(argc: usize, argv: *const Value) -> Value;

#[repr(C)]
pub struct ActorHandle {
    header: ObjHeader,
    class_id: u32,
    pool_size: i64,
    objective: u8,
    mailbox: Arc<Mailbox>,
    instance: Value,
}

#[repr(C)]
pub struct PoolHandle {
    header: ObjHeader,
    objective: u8,
    pool_size: usize,
    rr: AtomicUsize,
    handles: Vec<Value>,
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

struct Mailbox {
    sender: Mutex<Option<mpsc::Sender<Message>>>,
    len: AtomicUsize,
    closed: AtomicBool,
    paused: AtomicBool,
    pause_notify: Notify,
    pause_epoch: AtomicUsize,
    pause_ack: AtomicUsize,
    pause_ack_notify: Notify,
    enqueue_timeout: Duration,
    batch_limit: usize,
}

struct PendingState {
    lock: Mutex<Option<Value>>,
    notify: Notify,
    dropped: AtomicBool,
}

static METHODS: OnceLock<Mutex<HashMap<u32, HashMap<u32, MethodFn>>>> = OnceLock::new();
static ARGS_POOL: OnceLock<Mutex<Vec<Vec<Value>>>> = OnceLock::new();
static RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn method_registry() -> &'static Mutex<HashMap<u32, HashMap<u32, MethodFn>>> {
    METHODS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn args_pool() -> &'static Mutex<Vec<Vec<Value>>> {
    ARGS_POOL.get_or_init(|| Mutex::new(Vec::new()))
}

const ARGS_POOL_MAX_CAP: usize = 64;
const ARGS_POOL_MAX_LEN: usize = 128;

fn runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_time()
            .thread_name("wrela-rt")
            .build()
            .expect("wrela tokio runtime")
    })
}

fn block_on<F: Future>(fut: F) -> F::Output {
    if let Ok(handle) = Handle::try_current() {
        tokio::task::block_in_place(|| handle.block_on(fut))
    } else {
        runtime().block_on(fut)
    }
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
    let objective = normalize_objective(objective);
    let mut config = actor_config_for_objective(objective);
    if mailbox_cap > 0 {
        config.mailbox_cap = mailbox_cap as usize;
    }
    if enqueue_timeout_ms >= 0 {
        config.enqueue_timeout = Duration::from_millis(enqueue_timeout_ms as u64);
    }
    if batch_limit > 0 {
        config.batch_limit = batch_limit as usize;
    }
    let pool_size = normalize_pool_size(pool_size, objective);
    let (tx, rx) = mpsc::channel(config.mailbox_cap);
    let mailbox = Arc::new(Mailbox {
        sender: Mutex::new(Some(tx)),
        len: AtomicUsize::new(0),
        closed: AtomicBool::new(false),
        paused: AtomicBool::new(false),
        pause_notify: Notify::new(),
        pause_epoch: AtomicUsize::new(0),
        pause_ack: AtomicUsize::new(0),
        pause_ack_notify: Notify::new(),
        enqueue_timeout: config.enqueue_timeout,
        batch_limit: config.batch_limit,
    });
    unsafe {
        wr_rc_inc(instance);
    }
    runtime().spawn(actor_loop(class_id, mailbox.clone(), rx));
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

pub fn pool_new(handles_val: Value, objective: i64) -> Value {
    let list = match crate::list::as_list_ref(handles_val) {
        Some(list) => list,
        None => return Value::nil(),
    };
    let objective = normalize_objective(objective);
    let mut handles = Vec::new();
    unsafe {
        for handle in (*list).data.iter() {
            wr_rc_inc(*handle);
            handles.push(*handle);
        }
    }
    let pool_size = handles.len();
    let obj = Box::new(PoolHandle {
        header: header(TypeId::Pool),
        objective,
        pool_size,
        rr: AtomicUsize::new(0),
        handles,
    });
    Value::from_ptr(Box::into_raw(obj) as *mut ObjHeader)
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
        notify: Notify::new(),
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
        loop {
            let guard = p.state.lock.lock().expect("pending lock");
            if let Some(val) = *guard {
                return result::result_ok(val);
            }
            drop(guard);
            block_on(p.state.notify.notified());
        }
    }
}

pub unsafe fn drop_actor(ptr: *mut ObjHeader) {
    let actor = ptr as *mut ActorHandle;
    unsafe {
        let mailbox = (*actor).mailbox.clone();
        let instance = (*actor).instance;
        mailbox.closed.store(true, Ordering::Release);
        let sender = {
            let mut guard = mailbox.sender.lock().expect("mailbox sender lock");
            guard.take()
        };
        drop(sender);
        wr_rc_dec(instance);
        drop(Box::from_raw(actor));
    }
}

pub unsafe fn drop_pool(ptr: *mut ObjHeader) {
    let pool = ptr as *mut PoolHandle;
    unsafe {
        for handle in (&(*pool).handles).iter() {
            wr_rc_dec(*handle);
        }
        drop(Box::from_raw(pool));
    }
}

pub unsafe fn drop_pending(ptr: *mut ObjHeader) {
    let pending = unsafe { Box::from_raw(ptr as *mut PendingObj) };
    pending.state.dropped.store(true, Ordering::Release);
    inc_pending_dropped();
    let mut guard = pending.state.lock.lock().expect("pending lock");
    if let Some(val) = guard.take() {
        unsafe { wr_rc_dec(val) };
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

fn mailbox_len(actor: *const ActorHandle) -> usize {
    unsafe { (&(*actor).mailbox).len.load(Ordering::Relaxed) }
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
            block_on(mailbox.pause_ack_notify.notified());
        }
    }
}

fn enqueue_pause_message(actor: *const ActorHandle) {
    unsafe {
        let mailbox = (*actor).mailbox.clone();
        let args_vec = take_args_vec();
        let msg = Message {
            method_id: u32::MAX,
            args: args_vec,
            pending: None,
        };
        enqueue_message(mailbox, msg);
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

fn pool_send(pool: *const PoolHandle, method_id: u32, argc: usize, argv_ptr: *const Value) -> Value {
    unsafe {
        if (*pool).pool_size == 0 {
            return Value::nil();
        }
        let idx = (*pool).rr.fetch_add(1, Ordering::Relaxed) % (*pool).pool_size;
        let handle = (&(*pool).handles)[idx];
        actor_send(handle, method_id, argc, argv_ptr)
    }
}

fn pool_fire(pool: *const PoolHandle, method_id: u32, argc: usize, argv_ptr: *const Value) {
    unsafe {
        if (*pool).pool_size == 0 {
            return;
        }
        let idx = (*pool).rr.fetch_add(1, Ordering::Relaxed) % (*pool).pool_size;
        let handle = (&(*pool).handles)[idx];
        actor_fire(handle, method_id, argc, argv_ptr);
    }
}

fn enqueue_message(mailbox: Arc<Mailbox>, msg: Message) {
    if mailbox.closed.load(Ordering::Acquire) {
        drop_message(msg);
        inc_messages_dropped();
        return;
    }
    let sender = {
        let guard = mailbox.sender.lock().expect("mailbox sender lock");
        guard.clone()
    };
    let Some(sender) = sender else {
        drop_message(msg);
        inc_messages_dropped();
        return;
    };
    let timeout = mailbox.enqueue_timeout;
    let result = block_on(sender.send_timeout(msg, timeout));
    match result {
        Ok(()) => {
            let len = mailbox.len.fetch_add(1, Ordering::AcqRel) + 1;
            update_mailbox_high_water(len);
            inc_messages_sent();
        }
        Err(SendTimeoutError::Timeout(msg)) | Err(SendTimeoutError::Closed(msg)) => {
            drop_message(msg);
            inc_messages_dropped();
        }
    }
}

async fn actor_loop(class_id: u32, mailbox: Arc<Mailbox>, mut rx: mpsc::Receiver<Message>) {
    loop {
        let msg = match rx.recv().await {
            Some(msg) => msg,
            None => break,
        };
        while mailbox.paused.load(Ordering::Acquire) {
            let epoch = mailbox.pause_epoch.load(Ordering::Acquire);
            mailbox.pause_ack.store(epoch, Ordering::Release);
            mailbox.pause_ack_notify.notify_waiters();
            mailbox.pause_notify.notified().await;
        }
        if handle_message(&mailbox, class_id, msg, &mut rx).await {
            tokio::task::yield_now().await;
        } else {
            break;
        }
    }
}

async fn handle_message(
    mailbox: &Mailbox,
    class_id: u32,
    first: Message,
    rx: &mut mpsc::Receiver<Message>,
) -> bool {
    let batch_limit = mailbox.batch_limit.max(1);
    let mut current = first;
    for idx in 0..batch_limit {
        if mailbox.closed.load(Ordering::Acquire) {
            drop_message(current);
            mailbox.len.fetch_sub(1, Ordering::AcqRel);
            drain_messages(mailbox, rx);
            return false;
        }
        process_message(class_id, current);
        mailbox.len.fetch_sub(1, Ordering::AcqRel);
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

fn drain_messages(mailbox: &Mailbox, rx: &mut mpsc::Receiver<Message>) {
    loop {
        let msg = match rx.try_recv() {
            Ok(msg) => msg,
            Err(_) => break,
        };
        drop_message(msg);
        mailbox.len.fetch_sub(1, Ordering::AcqRel);
    }
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
        resolve_pending(pending, result);
    }
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

fn resolve_pending(pending: Arc<PendingState>, value: Value) {
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
    pending.notify.notify_waiters();
}
