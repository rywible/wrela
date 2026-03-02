//! Tokio-based actor system: singleton (`* 1`) and auto-scaled pool (`* n`).
//! Replaces the former custom scheduler with tokio::sync::mpsc and oneshot.

use crate::arena;
use crate::config;
use crate::data::object::drop_object;
use crate::kernel::runtime;
#[cfg(feature = "metrics")]
use crate::metrics::inc_alloc_pending;
use crate::metrics::{
    inc_mailbox_enqueue_fail, inc_messages_dropped, inc_messages_dropped_paused,
    inc_pending_dropped, inc_pending_resolved,
};
use crate::object::ObjHeader;
use crate::result;
use crate::value::{TypeId, Value, header};
use crate::wr_rc_dec;
use crate::wr_rc_inc;
use futures::executor::block_on as futures_block_on;
use std::collections::HashMap;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

pub type MethodFn = extern "C" fn(argc: usize, argv: *const Value) -> Value;

static METHOD_REGISTRY_GENERATION: AtomicU64 = AtomicU64::new(0);
static NEXT_POOL_ID: AtomicU64 = AtomicU64::new(1);

fn method_registry() -> &'static Mutex<HashMap<u32, HashMap<u32, MethodFn>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<u32, HashMap<u32, MethodFn>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn resolve_method(class_id: u32, method_id: u32) -> Option<MethodFn> {
    let map = method_registry().lock().expect("method registry lock");
    map.get(&class_id)
        .and_then(|methods| methods.get(&method_id).copied())
}

#[repr(C)]
pub struct ActorHandle {
    header: ObjHeader,
    class_id: u32,
    pool_size: i64,
    pub objective: u8,
    tx: mpsc::Sender<ActorMessage>,
    instance: Value,
}

#[repr(C)]
pub struct PoolHandle {
    header: ObjHeader,
    pub pool_id: u64,
    class_id: u32,
    tx: mpsc::Sender<ActorMessage>,
    pool_size: usize,
    rr: AtomicUsize,
    pub(crate) handles: Vec<Value>,
    alive: AtomicBool,
}

#[repr(C)]
pub struct PendingObj {
    header: ObjHeader,
    rx: Option<oneshot::Receiver<Value>>,
}

struct ActorMessage {
    method_id: u32,
    instance: Value,
    args: MessageArgs,
    pending: Option<oneshot::Sender<Value>>,
}

enum MessageArgs {
    None,
    Inline1(Value),
    Inline2(Value, Value),
    Heap(Vec<Value>),
}

impl MessageArgs {
    #[inline]
    #[allow(dead_code)]
    fn len(&self) -> usize {
        match self {
            Self::None => 0,
            Self::Inline1(_) => 1,
            Self::Inline2(_, _) => 2,
            Self::Heap(v) => v.len(),
        }
    }

    #[inline]
    unsafe fn rc_inc_all(&self) {
        match self {
            Self::None => {}
            Self::Inline1(a0) => unsafe {
                if a0.is_ptr() {
                    wr_rc_inc(*a0);
                }
            },
            Self::Inline2(a0, a1) => unsafe {
                if a0.is_ptr() {
                    wr_rc_inc(*a0);
                }
                if a1.is_ptr() {
                    wr_rc_inc(*a1);
                }
            },
            Self::Heap(v) => unsafe {
                for arg in v.iter().copied() {
                    if arg.is_ptr() {
                        wr_rc_inc(arg);
                    }
                }
            },
        }
    }

    #[inline]
    unsafe fn rc_dec_all(&self) {
        match self {
            Self::None => {}
            Self::Inline1(a0) => unsafe {
                if a0.is_ptr() {
                    wr_rc_dec(*a0);
                }
            },
            Self::Inline2(a0, a1) => unsafe {
                if a0.is_ptr() {
                    wr_rc_dec(*a0);
                }
                if a1.is_ptr() {
                    wr_rc_dec(*a1);
                }
            },
            Self::Heap(v) => unsafe {
                for arg in v.iter().copied() {
                    if arg.is_ptr() {
                        wr_rc_dec(arg);
                    }
                }
            },
        }
    }
}

const ARGS_POOL_MAX_LEN: usize = 64;
const ARGS_POOL_MAX_CAP: usize = 64;

fn args_pool() -> &'static Mutex<Vec<Vec<Value>>> {
    static POOL: OnceLock<Mutex<Vec<Vec<Value>>>> = OnceLock::new();
    POOL.get_or_init(|| Mutex::new(Vec::new()))
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

fn runtime_error(message: &str) {
    eprintln!("runtime error: {message}");
}

fn actor_capability_denied(operation: &str) {
    runtime_error(&format!("capability_denied:actor.{operation}"));
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
    pending: Option<oneshot::Sender<Value>>,
) -> Option<ActorMessage> {
    if arena::is_arena_value(instance) {
        crate::metrics::inc_message_instance_is_arena();
        return None;
    }
    for arg in args {
        if arena::is_arena_value(*arg) {
            return None;
        }
    }
    if instance.is_ptr() {
        unsafe { wr_rc_inc(instance) };
    }
    let args = match args.len() {
        0 => {
            crate::metrics::inc_message_build_noargs();
            MessageArgs::None
        }
        1 => {
            crate::metrics::inc_message_build_args();
            MessageArgs::Inline1(args[0])
        }
        2 => {
            crate::metrics::inc_message_build_args();
            MessageArgs::Inline2(args[0], args[1])
        }
        _ => {
            crate::metrics::inc_message_build_args();
            let mut args_vec = take_args_vec();
            args_vec.extend_from_slice(args);
            MessageArgs::Heap(args_vec)
        }
    };
    let msg = ActorMessage {
        method_id,
        instance,
        args,
        pending,
    };
    unsafe { msg.args.rc_inc_all() };
    Some(msg)
}

fn execute_message(class_id: u32, msg: ActorMessage, arena_guard: &mut arena::Arena) {
    let func = resolve_method(class_id, msg.method_id);
    if func.is_none() {
        crate::metrics::inc_actor_method_missing();
    }
    let catch_panic = config::actor_catch_panic_enabled();
    let result = if let Some(func) = func {
        match &msg.args {
            MessageArgs::None => {
                let argv_ptr: *const Value = &msg.instance;
                if catch_panic {
                    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        func(1, argv_ptr)
                    })) {
                        Ok(val) => val,
                        Err(_) => {
                            crate::metrics::inc_actor_method_panic();
                            Value::nil()
                        }
                    }
                } else {
                    func(1, argv_ptr)
                }
            }
            MessageArgs::Inline1(a0) => {
                let mut inline: [MaybeUninit<Value>; 2] = [const { MaybeUninit::uninit() }; 2];
                inline[0].write(msg.instance);
                inline[1].write(*a0);
                let argv_ptr = inline.as_ptr() as *const Value;
                if catch_panic {
                    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        func(2, argv_ptr)
                    })) {
                        Ok(val) => val,
                        Err(_) => {
                            crate::metrics::inc_actor_method_panic();
                            Value::nil()
                        }
                    }
                } else {
                    func(2, argv_ptr)
                }
            }
            MessageArgs::Inline2(a0, a1) => {
                let mut inline: [MaybeUninit<Value>; 3] = [const { MaybeUninit::uninit() }; 3];
                inline[0].write(msg.instance);
                inline[1].write(*a0);
                inline[2].write(*a1);
                let argv_ptr = inline.as_ptr() as *const Value;
                if catch_panic {
                    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        func(3, argv_ptr)
                    })) {
                        Ok(val) => val,
                        Err(_) => {
                            crate::metrics::inc_actor_method_panic();
                            Value::nil()
                        }
                    }
                } else {
                    func(3, argv_ptr)
                }
            }
            MessageArgs::Heap(args) => {
                let mut heap_argv: Vec<Value> = Vec::new();
                heap_argv.reserve(1 + args.len());
                heap_argv.push(msg.instance);
                heap_argv.extend_from_slice(args);
                if catch_panic {
                    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        func(heap_argv.len(), heap_argv.as_ptr())
                    })) {
                        Ok(val) => val,
                        Err(_) => {
                            crate::metrics::inc_actor_method_panic();
                            Value::nil()
                        }
                    }
                } else {
                    func(heap_argv.len(), heap_argv.as_ptr())
                }
            }
        }
    } else {
        Value::nil()
    };

    let result = arena::reject_arena_escape(result, "actor return").unwrap_or_default();
    arena_guard.reset();

    if result.is_ptr() {
        unsafe { wr_rc_inc(result) };
    }
    unsafe { msg.args.rc_dec_all() };
    if msg.instance.is_ptr() {
        unsafe { wr_rc_dec(msg.instance) };
    }
    if let MessageArgs::Heap(args) = msg.args {
        return_args_vec(args);
    }
    if let Some(tx) = msg.pending {
        match tx.send(result) {
            Ok(()) => inc_pending_resolved(),
            Err(value) => {
                if value.is_ptr() {
                    unsafe { wr_rc_dec(value) };
                }
            }
        }
    } else if result.is_ptr() {
        unsafe { wr_rc_dec(result) };
    }
}

fn drop_message(msg: ActorMessage) {
    if let Some(tx) = msg.pending {
        let _ = tx.send(Value::nil());
    }
    unsafe { msg.args.rc_dec_all() };
    if msg.instance.is_ptr() {
        unsafe { wr_rc_dec(msg.instance) };
    }
    if let MessageArgs::Heap(args) = msg.args {
        return_args_vec(args);
    }
}

pub fn register_method(class_id: u32, method_id: u32, func: MethodFn) {
    let mut map = method_registry().lock().expect("method registry lock");
    map.entry(class_id).or_default().insert(method_id, func);
    METHOD_REGISTRY_GENERATION.fetch_add(1, Ordering::Release);
}

pub fn actor_spawn(
    class_id: u64,
    instance: Value,
    pool_size: i64,
    objective: i64,
    mailbox_cap: i64,
    _enqueue_timeout_ms: i64,
    batch_limit: i64,
) -> Value {
    if !config::capability_actor_enabled() {
        actor_capability_denied("spawn");
        return Value::nil();
    }
    let class_id = class_id as u32;
    let config = config::actor_config();
    let cap = if mailbox_cap > 0 {
        mailbox_cap as usize
    } else {
        config.mailbox_cap
    };
    let cap = cap.max(1);
    let _batch_limit = if batch_limit > 0 {
        batch_limit as usize
    } else {
        config.batch_limit
    }
    .max(1);

    let Some(pool_size) = (pool_size > 0).then_some(pool_size) else {
        runtime_error("actor_spawn: pool_size must be > 0");
        return Value::nil();
    };

    if objective == 7 {
        return Value::nil();
    }

    let instance = crate::class::promote_user_object_to_heap(instance);
    if instance.is_ptr() {
        crate::metrics::inc_actor_spawn_instance_is_ptr();
    } else {
        crate::metrics::inc_actor_spawn_instance_not_ptr();
    }
    unsafe {
        wr_rc_inc(instance);
    }

    let (tx, mut rx) = mpsc::channel(cap);
    let deterministic_runtime = config::deterministic_runtime_enabled();

    if pool_size == 1 {
        let instance_clone = instance;
        runtime::tokio_runtime().spawn(async move {
            let mut arena = arena::Arena::new(64 * 1024);
            while let Some(msg) = rx.recv().await {
                let arena_ptr = &mut arena as *mut arena::Arena;
                let _guard = arena::enter(arena_ptr);
                execute_message(class_id, msg, unsafe { &mut *arena_ptr });
            }
        });

        let obj = Box::new(ActorHandle {
            header: header(TypeId::Actor),
            class_id,
            pool_size,
            objective: 0,
            tx,
            instance: instance_clone,
        });
        Value::from_ptr(Box::into_raw(obj) as *mut ObjHeader)
    } else {
        if instance.is_ptr() {
            unsafe { wr_rc_dec(instance) };
        }
        runtime::tokio_runtime().spawn(async move {
            if deterministic_runtime {
                let mut arena = arena::Arena::new(64 * 1024);
                while let Some(msg) = rx.recv().await {
                    let arena_ptr = &mut arena as *mut arena::Arena;
                    let _guard = arena::enter(arena_ptr);
                    execute_message(class_id, msg, unsafe { &mut *arena_ptr });
                }
            } else {
                while let Some(msg) = rx.recv().await {
                    let _ = tokio::task::spawn_blocking(move || {
                        let mut arena = arena::Arena::new(64 * 1024);
                        let arena_ptr = &mut arena as *mut arena::Arena;
                        let _guard = arena::enter(arena_ptr);
                        execute_message(class_id, msg, unsafe { &mut *arena_ptr });
                    })
                    .await;
                }
            }
        });

        let pool_id = NEXT_POOL_ID.fetch_add(1, Ordering::Relaxed);
        let obj = Box::new(PoolHandle {
            header: header(TypeId::Pool),
            pool_id,
            class_id,
            tx,
            pool_size: pool_size as usize,
            rr: AtomicUsize::new(0),
            handles: vec![],
            alive: AtomicBool::new(true),
        });
        Value::from_ptr(Box::into_raw(obj) as *mut ObjHeader)
    }
}

pub fn pool_new(
    handles_val: Value,
    _objective: i64,
    _min_size: i64,
    _max_size: i64,
    _weight: i64,
    queue_cap: i64,
) -> Value {
    if !config::capability_actor_enabled() {
        actor_capability_denied("pool_new");
        return Value::nil();
    }
    let list = match crate::list::as_list_ref(handles_val) {
        Some(list) => list,
        None => return Value::nil(),
    };

    let cap = if queue_cap > 0 {
        queue_cap as usize
    } else {
        config::actor_config().mailbox_cap
    };
    let cap = cap.max(1);

    let first_handle = match unsafe { (*list).data.first().copied() } {
        Some(h) => h,
        None => return Value::nil(),
    };
    let actor = match as_actor(first_handle) {
        Some(a) => a,
        None => return Value::nil(),
    };
    let class_id = unsafe { (*actor).class_id };

    let (tx, mut rx) = mpsc::channel(cap);

    let mut handles = Vec::new();
    unsafe {
        for handle in (*list).data.iter() {
            wr_rc_inc(*handle);
            handles.push(*handle);
        }
    }
    let pool_size = handles.len();
    let deterministic_runtime = config::deterministic_runtime_enabled();

    runtime::tokio_runtime().spawn(async move {
        if pool_size == 1 {
            let mut arena = arena::Arena::new(64 * 1024);
            while let Some(msg) = rx.recv().await {
                let arena_ptr = &mut arena as *mut arena::Arena;
                let _guard = arena::enter(arena_ptr);
                execute_message(class_id, msg, unsafe { &mut *arena_ptr });
            }
        } else if deterministic_runtime {
            let mut arena = arena::Arena::new(64 * 1024);
            while let Some(msg) = rx.recv().await {
                let arena_ptr = &mut arena as *mut arena::Arena;
                let _guard = arena::enter(arena_ptr);
                execute_message(class_id, msg, unsafe { &mut *arena_ptr });
            }
        } else {
            while let Some(msg) = rx.recv().await {
                let _ = tokio::task::spawn_blocking(move || {
                    let mut arena = arena::Arena::new(64 * 1024);
                    let arena_ptr = &mut arena as *mut arena::Arena;
                    let _guard = arena::enter(arena_ptr);
                    execute_message(class_id, msg, unsafe { &mut *arena_ptr });
                })
                .await;
            }
        }
    });

    let pool_id = NEXT_POOL_ID.fetch_add(1, Ordering::Relaxed);

    let obj = Box::new(PoolHandle {
        header: header(TypeId::Pool),
        pool_id,
        class_id,
        tx,
        pool_size,
        rr: AtomicUsize::new(0),
        handles,
        alive: AtomicBool::new(true),
    });
    Value::from_ptr(Box::into_raw(obj) as *mut ObjHeader)
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

pub(crate) fn actor_backing_instance(handle: Value) -> Option<Value> {
    let actor = as_actor(handle)?;
    unsafe { Some((*actor).instance) }
}

pub fn actor_send(handle: Value, method_id: u32, argc: usize, argv_ptr: *const Value) -> Value {
    if !config::capability_actor_enabled() {
        actor_capability_denied("send");
        return Value::nil();
    }
    if let Some(pool) = as_pool_ref(handle) {
        return pool_send(pool, method_id, argc, argv_ptr);
    }
    let actor = match as_actor(handle) {
        Some(actor) => actor,
        None => return Value::nil(),
    };

    let args = match unsafe { args_from_raw(argc, argv_ptr) } {
        Some(a) => a,
        None => return Value::nil(),
    };
    let (tx, rx) = oneshot::channel();
    let msg = match build_message_inner(unsafe { (*actor).instance }, method_id, args, Some(tx)) {
        Some(m) => m,
        None => return Value::nil(),
    };

    #[cfg(feature = "metrics")]
    inc_alloc_pending();

    let pending = Box::new(PendingObj {
        header: header(TypeId::Pending),
        rx: Some(rx),
    });

    let actor_tx = unsafe { (*actor).tx.clone() };
    if let Err(e) = actor_tx.try_send(msg) {
        drop_message(e.into_inner());
        let pending = Box::into_raw(pending) as *mut ObjHeader;
        unsafe { drop_object(pending) };
        inc_mailbox_enqueue_fail();
        inc_messages_dropped();
        return Value::nil();
    }

    crate::metrics::inc_mailbox_enqueue_ok();
    crate::metrics::inc_messages_sent();

    Value::from_ptr(Box::into_raw(pending) as *mut ObjHeader)
}

pub fn actor_fire(handle: Value, method_id: u32, argc: usize, argv_ptr: *const Value) {
    if !config::capability_actor_enabled() {
        actor_capability_denied("fire");
        return;
    }
    if let Some(pool) = as_pool_ref(handle) {
        pool_fire(pool, method_id, argc, argv_ptr);
        return;
    }
    let actor = match as_actor(handle) {
        Some(actor) => actor,
        None => return,
    };

    let args = match unsafe { args_from_raw(argc, argv_ptr) } {
        Some(a) => a,
        None => return,
    };
    let msg = match build_message_inner(unsafe { (*actor).instance }, method_id, args, None) {
        Some(m) => m,
        None => return,
    };

    let actor_tx = unsafe { (*actor).tx.clone() };
    if let Err(e) = actor_tx.try_send(msg) {
        drop_message(e.into_inner());
        inc_mailbox_enqueue_fail();
        inc_messages_dropped();
    } else {
        crate::metrics::inc_mailbox_enqueue_ok();
        crate::metrics::inc_messages_sent();
    }
}

fn pool_send(
    pool: *const PoolHandle,
    method_id: u32,
    argc: usize,
    argv_ptr: *const Value,
) -> Value {
    unsafe {
        if !(*pool).alive.load(Ordering::Acquire) {
            inc_messages_dropped_paused();
            return Value::nil();
        }

        let pool_size = (*pool).handles.len();
        let actor = if pool_size == 0 {
            None
        } else {
            let index = (*pool).rr.fetch_add(1, Ordering::Relaxed) % pool_size;
            as_actor((&(*pool).handles)[index])
        };
        let instance = actor.map_or(Value::nil(), |actor| (*actor).instance);
        let args = match args_from_raw(argc, argv_ptr) {
            Some(a) => a,
            None => return Value::nil(),
        };
        let (tx, rx) = oneshot::channel();
        let msg = match build_message_inner(instance, method_id, args, Some(tx)) {
            Some(m) => m,
            None => return Value::nil(),
        };

        #[cfg(feature = "metrics")]
        inc_alloc_pending();

        let pending = Box::new(PendingObj {
            header: header(TypeId::Pending),
            rx: Some(rx),
        });

        let send_result = if let Some(actor) = actor {
            (*actor).tx.clone().try_send(msg)
        } else {
            (*pool).tx.clone().try_send(msg)
        };
        if let Err(e) = send_result {
            drop_message(e.into_inner());
            let pending = Box::into_raw(pending) as *mut ObjHeader;
            drop_object(pending);
            inc_mailbox_enqueue_fail();
            inc_messages_dropped();
            return Value::nil();
        }

        crate::metrics::inc_mailbox_enqueue_ok();
        crate::metrics::inc_messages_sent();

        Value::from_ptr(Box::into_raw(pending) as *mut ObjHeader)
    }
}

fn pool_fire(pool: *const PoolHandle, method_id: u32, argc: usize, argv_ptr: *const Value) {
    unsafe {
        if !(*pool).alive.load(Ordering::Acquire) {
            inc_messages_dropped_paused();
            return;
        }

        let pool_size = (*pool).handles.len();
        let actor = if pool_size == 0 {
            None
        } else {
            let index = (*pool).rr.fetch_add(1, Ordering::Relaxed) % pool_size;
            as_actor((&(*pool).handles)[index])
        };
        let instance = actor.map_or(Value::nil(), |actor| (*actor).instance);
        let args = match args_from_raw(argc, argv_ptr) {
            Some(a) => a,
            None => return,
        };
        let msg = match build_message_inner(instance, method_id, args, None) {
            Some(m) => m,
            None => return,
        };

        let send_result = if let Some(actor) = actor {
            (*actor).tx.clone().try_send(msg)
        } else {
            (*pool).tx.clone().try_send(msg)
        };
        if let Err(e) = send_result {
            drop_message(e.into_inner());
            inc_mailbox_enqueue_fail();
            inc_messages_dropped();
        } else {
            crate::metrics::inc_mailbox_enqueue_ok();
            crate::metrics::inc_messages_sent();
        }
    }
}

pub fn pending_await(pending: Value) -> Value {
    if !config::capability_actor_enabled() {
        actor_capability_denied("await");
        return Value::nil();
    }
    if !pending.is_ptr() {
        return Value::nil();
    }
    unsafe {
        let header = &*pending.as_ptr();
        if header.type_id != TypeId::Pending as u32 {
            return Value::nil();
        }
        let p = &mut *(pending.as_ptr() as *mut PendingObj);
        let rx = p.rx.take();
        match rx {
            Some(r) => {
                // Use futures::block_on to avoid panicking when called from within
                // a Tokio context (e.g. spawn_blocking). oneshot::Receiver is a
                // Future that does not require Tokio to drive.
                let val = futures_block_on(r).unwrap_or(Value::nil());
                let wrapped = result::result_ok(val);
                if val.is_ptr() {
                    wr_rc_dec(val);
                }
                wrapped
            }
            None => result::result_ok(Value::nil()),
        }
    }
}

pub fn sleep_ms(ms: i64) -> Value {
    if ms <= 0 {
        return result::result_ok(Value::nil());
    }
    let (tx, rx) = oneshot::channel();
    let _ = runtime::tokio_runtime().spawn(async move {
        tokio::time::sleep(Duration::from_millis(ms as u64)).await;
        let _ = tx.send(Value::nil());
    });
    // Use futures::block_on to avoid panicking when called from within a Tokio context.
    let v = futures_block_on(rx).unwrap_or(Value::nil());
    result::result_ok(v)
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
        unsafe {
            return Value::from_int((*pool).rr.load(Ordering::Relaxed) as i64);
        }
    }
    Value::nil()
}

#[allow(dead_code)]
pub fn pool_queue_len(_handle: Value) -> Value {
    Value::from_int(0)
}

#[allow(dead_code)]
pub fn actor_mailbox_len(_handle: Value) -> Value {
    Value::from_int(0)
}

#[allow(dead_code)]
pub fn actor_pause(_handle: Value) -> Value {
    Value::nil()
}

#[allow(dead_code)]
pub fn actor_resume(_handle: Value) -> Value {
    Value::nil()
}

#[allow(dead_code)]
pub fn actor_pause_wait(_handle: Value) -> Value {
    Value::nil()
}

#[allow(dead_code)]
pub fn actor_fire_burst_begin(_handle: Value) {}

#[allow(dead_code)]
pub fn actor_fire_burst_end(_handle: Value) {}

#[allow(dead_code)]
pub fn actor_fire_burst_abort(_handle: Value) {}

pub unsafe fn drop_actor(ptr: *mut ObjHeader) {
    let actor = unsafe { Box::from_raw(ptr as *mut ActorHandle) };
    if actor.instance.is_ptr() {
        unsafe { wr_rc_dec(actor.instance) };
    }
}

pub unsafe fn drop_pool(ptr: *mut ObjHeader) {
    let mut pool = unsafe { Box::from_raw(ptr as *mut PoolHandle) };
    for handle in pool.handles.drain(..) {
        if handle.is_ptr() {
            unsafe { wr_rc_dec(handle) };
        }
    }
}

pub unsafe fn drop_pending(ptr: *mut ObjHeader) {
    let mut pending = unsafe { Box::from_raw(ptr as *mut PendingObj) };
    if let Some(mut rx) = pending.rx.take() {
        if let Ok(value) = rx.try_recv() {
            if value.is_ptr() {
                unsafe { wr_rc_dec(value) };
            }
        }
    }
    inc_pending_dropped();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{RuntimeConfig, set_test_runtime_config_override};
    use std::sync::{Mutex, MutexGuard, OnceLock};
    use std::time::Duration;

    fn runtime_override_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct CapabilityOverrideGuard {
        _lock: MutexGuard<'static, ()>,
    }

    impl CapabilityOverrideGuard {
        fn install(config: RuntimeConfig) -> Self {
            let lock = runtime_override_lock()
                .lock()
                .unwrap_or_else(|err| err.into_inner());
            set_test_runtime_config_override(Some(config));
            Self { _lock: lock }
        }
    }

    impl Drop for CapabilityOverrideGuard {
        fn drop(&mut self) {
            set_test_runtime_config_override(None);
        }
    }

    fn deterministic_order_log() -> &'static Mutex<Vec<i64>> {
        static LOG: OnceLock<Mutex<Vec<i64>>> = OnceLock::new();
        LOG.get_or_init(|| Mutex::new(Vec::new()))
    }

    extern "C" fn record_order_and_echo_method(argc: usize, argv: *const Value) -> Value {
        if argc >= 2 && !argv.is_null() {
            let args = unsafe { std::slice::from_raw_parts(argv, argc) };
            if args[1].is_int() {
                deterministic_order_log()
                    .lock()
                    .expect("order log lock")
                    .push(args[1].as_int());
                return args[1];
            }
        }
        Value::nil()
    }

    fn wait_for_expected_order(expected: &[i64]) -> Vec<i64> {
        for _ in 0..50 {
            let current = deterministic_order_log()
                .lock()
                .expect("order log lock")
                .clone();
            if current == expected {
                return current;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        deterministic_order_log()
            .lock()
            .expect("order log lock")
            .clone()
    }

    #[test]
    fn actor_spawn_is_blocked_when_actor_capability_disabled() {
        let mut cfg = RuntimeConfig::default();
        cfg.allow_actor = false;
        let _guard = CapabilityOverrideGuard::install(cfg);

        let handle = actor_spawn(1, Value::nil(), 1, 0, 0, 0, 0);
        assert!(handle.is_nil());
    }

    #[test]
    fn actor_spawn_succeeds_when_actor_capability_enabled() {
        let mut cfg = RuntimeConfig::default();
        cfg.allow_actor = true;
        let _guard = CapabilityOverrideGuard::install(cfg);

        let handle = actor_spawn(1, Value::nil(), 1, 0, 0, 0, 0);
        assert!(handle.is_ptr(), "expected actor handle pointer");
        unsafe {
            crate::wr_rc_dec(handle);
        }
    }

    #[test]
    fn pool_new_is_blocked_when_actor_capability_disabled() {
        let mut allow_cfg = RuntimeConfig::default();
        allow_cfg.allow_actor = true;
        let _allow_guard = CapabilityOverrideGuard::install(allow_cfg);
        let handle = actor_spawn(1, Value::nil(), 1, 0, 0, 0, 0);
        assert!(handle.is_ptr(), "expected actor handle pointer");

        let list = crate::list::list_new(0);
        crate::list::list_push(list, handle);

        drop(_allow_guard);
        let mut deny_cfg = RuntimeConfig::default();
        deny_cfg.allow_actor = false;
        let _deny_guard = CapabilityOverrideGuard::install(deny_cfg);

        let pool = pool_new(list, 0, 0, 0, 0, 0);
        assert!(pool.is_nil(), "expected pool creation to be denied");

        unsafe {
            crate::wr_rc_dec(list);
            crate::wr_rc_dec(handle);
        }
    }

    #[test]
    fn actor_send_is_blocked_when_actor_capability_disabled() {
        let mut allow_cfg = RuntimeConfig::default();
        allow_cfg.allow_actor = true;
        let _allow_guard = CapabilityOverrideGuard::install(allow_cfg);
        let handle = actor_spawn(1, Value::nil(), 1, 0, 0, 0, 0);
        assert!(handle.is_ptr(), "expected actor handle pointer");

        drop(_allow_guard);
        let mut deny_cfg = RuntimeConfig::default();
        deny_cfg.allow_actor = false;
        let _deny_guard = CapabilityOverrideGuard::install(deny_cfg);

        let pending = actor_send(handle, 1, 0, std::ptr::null());
        assert!(pending.is_nil(), "expected actor_send to be denied");

        unsafe {
            crate::wr_rc_dec(handle);
        }
    }

    #[test]
    fn actor_fire_is_noop_when_actor_capability_disabled() {
        let mut allow_cfg = RuntimeConfig::default();
        allow_cfg.allow_actor = true;
        let _allow_guard = CapabilityOverrideGuard::install(allow_cfg);
        let handle = actor_spawn(1, Value::nil(), 1, 0, 0, 0, 0);
        assert!(handle.is_ptr(), "expected actor handle pointer");

        drop(_allow_guard);
        let mut deny_cfg = RuntimeConfig::default();
        deny_cfg.allow_actor = false;
        let _deny_guard = CapabilityOverrideGuard::install(deny_cfg);

        actor_fire(handle, 1, 0, std::ptr::null());

        unsafe {
            crate::wr_rc_dec(handle);
        }
    }

    #[test]
    fn pending_await_is_blocked_when_actor_capability_disabled() {
        let mut allow_cfg = RuntimeConfig::default();
        allow_cfg.allow_actor = true;
        let _allow_guard = CapabilityOverrideGuard::install(allow_cfg);
        let handle = actor_spawn(1, Value::nil(), 1, 0, 0, 0, 0);
        assert!(handle.is_ptr(), "expected actor handle pointer");
        let pending = actor_send(handle, 1, 0, std::ptr::null());
        assert!(
            pending.is_ptr(),
            "expected pending pointer while actor capability is enabled"
        );

        drop(_allow_guard);
        let mut deny_cfg = RuntimeConfig::default();
        deny_cfg.allow_actor = false;
        let _deny_guard = CapabilityOverrideGuard::install(deny_cfg);

        let awaited = pending_await(pending);
        assert!(awaited.is_nil(), "expected pending_await to be denied");

        unsafe {
            crate::wr_rc_dec(pending);
            crate::wr_rc_dec(handle);
        }
    }

    #[test]
    fn actor_pool_preserves_send_order_in_deterministic_mode() {
        let mut cfg = RuntimeConfig::default();
        cfg.allow_actor = true;
        cfg.deterministic = true;
        let _guard = CapabilityOverrideGuard::install(cfg);

        deterministic_order_log()
            .lock()
            .expect("order log lock")
            .clear();

        let class_id = 7_771u64;
        let method_id = 9u32;
        register_method(class_id as u32, method_id, record_order_and_echo_method);

        let handle = actor_spawn(class_id, Value::nil(), 4, 0, 0, 0, 0);
        assert!(handle.is_ptr(), "expected actor handle pointer");

        for value in 0..20i64 {
            let arg = Value::from_int(value);
            actor_fire(handle, method_id, 1, &arg as *const Value);
        }

        let expected: Vec<i64> = (0..20).collect();
        let observed = wait_for_expected_order(&expected);
        unsafe {
            crate::wr_rc_dec(handle);
        }
        assert_eq!(
            observed, expected,
            "expected deterministic pool send order in deterministic mode"
        );
    }

    #[test]
    fn deterministic_actor_order_is_stable_for_fire_send_and_await_across_pool_sizes() {
        let mut cfg = RuntimeConfig::default();
        cfg.allow_actor = true;
        cfg.deterministic = true;
        let _guard = CapabilityOverrideGuard::install(cfg);

        let class_id = 7_772u64;
        let method_id = 10u32;
        register_method(class_id as u32, method_id, record_order_and_echo_method);

        for pool_size in [1i64, 2i64, 4i64] {
            for iteration in 0..3 {
                deterministic_order_log()
                    .lock()
                    .expect("order log lock")
                    .clear();
                let handle = actor_spawn(class_id, Value::nil(), pool_size, 0, 0, 0, 0);
                assert!(handle.is_ptr(), "expected actor handle pointer");

                let mut expected = Vec::new();
                let mut pending_handles = Vec::new();
                for value in 0..10i64 {
                    expected.push(value);
                    let arg = Value::from_int(value);
                    actor_fire(handle, method_id, 1, &arg as *const Value);
                }
                for value in 10..20i64 {
                    expected.push(value);
                    let arg = Value::from_int(value);
                    let pending = actor_send(handle, method_id, 1, &arg as *const Value);
                    assert!(
                        pending.is_ptr(),
                        "expected pending for send in deterministic mode"
                    );
                    pending_handles.push((pending, value));
                }

                for (pending, expected_value) in pending_handles {
                    let awaited = pending_await(pending);
                    assert!(
                        crate::result::result_is_ok(awaited).as_bool(),
                        "pending_await should resolve successfully"
                    );
                    let resolved = crate::result::result_unwrap(awaited);
                    assert!(resolved.is_int(), "expected integer method result");
                    assert_eq!(
                        resolved.as_int(),
                        expected_value,
                        "expected echoed method result"
                    );
                    unsafe {
                        crate::wr_rc_dec(resolved);
                        crate::wr_rc_dec(awaited);
                        crate::wr_rc_dec(pending);
                    }
                }

                let observed = wait_for_expected_order(&expected);
                unsafe {
                    crate::wr_rc_dec(handle);
                }
                assert_eq!(
                    observed, expected,
                    "expected deterministic execution order for mixed fire/send/await pool_size={pool_size} iteration={iteration}"
                );
            }
        }
    }
}
