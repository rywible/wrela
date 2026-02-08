#![allow(clippy::missing_safety_doc)]

mod actor;
pub mod arena;
mod bytes;
mod class;
mod config;
mod diagnostics;
mod iter;
mod list;
mod logging;
mod map;
mod metrics;
mod object;
pub mod reactor;
mod result;
mod scheduler;
mod string;
mod value;

use value::int_value;
pub use value::{TypeId, Value};

use object::drop_object;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

const WR_REACTOR_EVENT_READABLE: i32 = 1;
const WR_REACTOR_EVENT_TIMER: i32 = 2;

struct ReactorRegistry {
    next_handle: AtomicU64,
    handles: Mutex<HashMap<u64, Arc<reactor::Reactor>>>,
}

impl ReactorRegistry {
    fn new() -> Self {
        Self {
            next_handle: AtomicU64::new(1),
            handles: Mutex::new(HashMap::new()),
        }
    }

    fn insert(&self, reactor: reactor::Reactor) -> i64 {
        let handle = self.next_handle.fetch_add(1, Ordering::Relaxed);
        self.handles
            .lock()
            .expect("reactor registry lock")
            .insert(handle, Arc::new(reactor));
        handle as i64
    }

    fn get(&self, handle: i64) -> Option<Arc<reactor::Reactor>> {
        if handle <= 0 {
            return None;
        }
        self.handles
            .lock()
            .expect("reactor registry lock")
            .get(&(handle as u64))
            .cloned()
    }

    fn remove(&self, handle: i64) -> bool {
        if handle <= 0 {
            return false;
        }
        self.handles
            .lock()
            .expect("reactor registry lock")
            .remove(&(handle as u64))
            .is_some()
    }
}

fn reactor_registry() -> &'static ReactorRegistry {
    static REGISTRY: OnceLock<ReactorRegistry> = OnceLock::new();
    REGISTRY.get_or_init(ReactorRegistry::new)
}

struct TaskSignalRegistry {
    next_handle: AtomicU64,
    handles: Mutex<HashMap<u64, Arc<reactor::task::TaskSignal>>>,
}

impl TaskSignalRegistry {
    fn new() -> Self {
        Self {
            next_handle: AtomicU64::new(1),
            handles: Mutex::new(HashMap::new()),
        }
    }

    fn insert(&self, signal: reactor::task::TaskSignal) -> i64 {
        let handle = self.next_handle.fetch_add(1, Ordering::Relaxed);
        self.handles
            .lock()
            .expect("task signal registry lock")
            .insert(handle, Arc::new(signal));
        handle as i64
    }

    fn get(&self, handle: i64) -> Option<Arc<reactor::task::TaskSignal>> {
        if handle <= 0 {
            return None;
        }
        self.handles
            .lock()
            .expect("task signal registry lock")
            .get(&(handle as u64))
            .cloned()
    }

    fn remove(&self, handle: i64) -> bool {
        if handle <= 0 {
            return false;
        }
        self.handles
            .lock()
            .expect("task signal registry lock")
            .remove(&(handle as u64))
            .is_some()
    }
}

fn task_signal_registry() -> &'static TaskSignalRegistry {
    static REGISTRY: OnceLock<TaskSignalRegistry> = OnceLock::new();
    REGISTRY.get_or_init(TaskSignalRegistry::new)
}

struct AtomicI64Registry {
    next_handle: AtomicU64,
    handles: Mutex<HashMap<u64, Arc<AtomicI64>>>,
}

impl AtomicI64Registry {
    fn new() -> Self {
        Self {
            next_handle: AtomicU64::new(1),
            handles: Mutex::new(HashMap::new()),
        }
    }

    fn insert(&self, value: i64) -> i64 {
        let handle = self.next_handle.fetch_add(1, Ordering::Relaxed);
        self.handles
            .lock()
            .expect("atomic registry lock")
            .insert(handle, Arc::new(AtomicI64::new(value)));
        handle as i64
    }

    fn get(&self, handle: i64) -> Option<Arc<AtomicI64>> {
        if handle <= 0 {
            return None;
        }
        self.handles
            .lock()
            .expect("atomic registry lock")
            .get(&(handle as u64))
            .cloned()
    }

    fn remove(&self, handle: i64) -> bool {
        if handle <= 0 {
            return false;
        }
        self.handles
            .lock()
            .expect("atomic registry lock")
            .remove(&(handle as u64))
            .is_some()
    }
}

fn atomic_i64_registry() -> &'static AtomicI64Registry {
    static REGISTRY: OnceLock<AtomicI64Registry> = OnceLock::new();
    REGISTRY.get_or_init(AtomicI64Registry::new)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wr_rc_inc(value: Value) {
    if !value.is_ptr() {
        return;
    }
    metrics::inc_rc_inc();
    let header = unsafe { &*value.as_ptr() };
    header.rc.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wr_rc_dec(value: Value) {
    if !value.is_ptr() {
        return;
    }
    metrics::inc_rc_dec();
    let header = unsafe { &*value.as_ptr() };
    let prev = header.rc.fetch_sub(1, std::sync::atomic::Ordering::Release);
    if prev == 0 {
        header.rc.store(0, std::sync::atomic::Ordering::Relaxed);
        return;
    }
    if prev == 1 {
        std::sync::atomic::fence(std::sync::atomic::Ordering::Acquire);
        if arena::is_arena_ptr(value.as_ptr()) {
            arena::drop_object_in_arena(value.as_ptr());
            return;
        }
        unsafe { drop_object(value.as_ptr()) };
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_box_float(val: f64) -> Value {
    Value::from_float(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_unbox_float(val: Value) -> f64 {
    if val.is_float() { val.as_float() } else { 0.0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_box_int(val: i64) -> Value {
    Value::from_int(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_unbox_int(val: Value) -> i64 {
    int_value(val).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_runtime_init() {
    diagnostics::runtime_init();
    #[cfg(feature = "metrics")]
    metrics::install_dump_hook();
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_runtime_abi() -> u32 {
    diagnostics::runtime_init();
    diagnostics::RUNTIME_ABI_VERSION
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_reactor_new() -> Value {
    match reactor::Reactor::new() {
        Ok(reactor) => Value::from_int(reactor_registry().insert(reactor)),
        Err(_) => Value::nil(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_reactor_drop(handle: Value) -> Value {
    let handle = int_value(handle).unwrap_or(0);
    Value::from_bool(reactor_registry().remove(handle))
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_reactor_register(handle: Value, token: Value) -> Value {
    let Some(reactor) = reactor_registry().get(int_value(handle).unwrap_or(0)) else {
        return Value::from_bool(false);
    };
    Value::from_bool(
        reactor
            .register(int_value(token).unwrap_or(0) as u64)
            .is_ok(),
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_reactor_deregister(handle: Value, token: Value) -> Value {
    let Some(reactor) = reactor_registry().get(int_value(handle).unwrap_or(0)) else {
        return Value::from_bool(false);
    };
    Value::from_bool(
        reactor
            .deregister(int_value(token).unwrap_or(0) as u64)
            .is_ok(),
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_reactor_arm_timer(handle: Value, token: Value, timeout_ms: Value) -> Value {
    let Some(reactor) = reactor_registry().get(int_value(handle).unwrap_or(0)) else {
        return Value::from_bool(false);
    };
    let token = int_value(token).unwrap_or(0) as u64;
    let timeout_ms = int_value(timeout_ms).unwrap_or(-1);
    Value::from_bool(reactor.arm_timer_ms(token, timeout_ms).is_ok())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wr_reactor_poll(
    handle: i64,
    timeout_ms: i64,
    out_token: *mut u64,
    out_kind: *mut i32,
) -> i32 {
    if out_token.is_null() || out_kind.is_null() {
        return -1;
    }
    let Some(reactor) = reactor_registry().get(handle) else {
        return -1;
    };
    match reactor.poll(timeout_ms) {
        Ok(Some(event)) => {
            unsafe {
                *out_token = event.token;
                *out_kind = match event.kind {
                    reactor::ReactorEventKind::Readable => WR_REACTOR_EVENT_READABLE,
                    reactor::ReactorEventKind::Timer => WR_REACTOR_EVENT_TIMER,
                };
            }
            1
        }
        Ok(None) => 0,
        Err(_) => -1,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_task_signal_new() -> Value {
    Value::from_int(task_signal_registry().insert(reactor::task::TaskSignal::new()))
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_task_signal_drop(handle: Value) -> Value {
    Value::from_bool(task_signal_registry().remove(int_value(handle).unwrap_or(0)))
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_task_unpark_one(handle: Value) -> Value {
    let Some(signal) = task_signal_registry().get(int_value(handle).unwrap_or(0)) else {
        return Value::from_bool(false);
    };
    signal.notify_one();
    Value::from_bool(true)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_task_unpark_all(handle: Value) -> Value {
    let Some(signal) = task_signal_registry().get(int_value(handle).unwrap_or(0)) else {
        return Value::from_bool(false);
    };
    signal.notify_waiters();
    Value::from_bool(true)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wr_task_park(
    handle: i64,
    observed_epoch: u64,
    timeout_ms: i64,
    out_epoch: *mut u64,
) -> i32 {
    if out_epoch.is_null() || timeout_ms < 0 {
        return -1;
    }
    let Some(signal) = task_signal_registry().get(handle) else {
        return -1;
    };
    let timeout = std::time::Duration::from_millis(timeout_ms as u64);
    let (epoch, notified) = signal.wait_timeout(observed_epoch, timeout);
    unsafe {
        *out_epoch = epoch;
    }
    if notified { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_task_epoch(handle: Value) -> Value {
    let Some(signal) = task_signal_registry().get(int_value(handle).unwrap_or(0)) else {
        return Value::nil();
    };
    Value::from_int(signal.snapshot() as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_atomic_i64_new(initial: Value) -> Value {
    Value::from_int(atomic_i64_registry().insert(int_value(initial).unwrap_or(0)))
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_atomic_i64_drop(handle: Value) -> Value {
    Value::from_bool(atomic_i64_registry().remove(int_value(handle).unwrap_or(0)))
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_atomic_i64_load(handle: Value) -> Value {
    let Some(cell) = atomic_i64_registry().get(int_value(handle).unwrap_or(0)) else {
        return Value::nil();
    };
    Value::from_int(cell.load(Ordering::SeqCst))
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_atomic_i64_store(handle: Value, value: Value) -> Value {
    let Some(cell) = atomic_i64_registry().get(int_value(handle).unwrap_or(0)) else {
        return Value::from_bool(false);
    };
    cell.store(int_value(value).unwrap_or(0), Ordering::SeqCst);
    Value::from_bool(true)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_atomic_i64_fetch_add(handle: Value, delta: Value) -> Value {
    let Some(cell) = atomic_i64_registry().get(int_value(handle).unwrap_or(0)) else {
        return Value::nil();
    };
    Value::from_int(cell.fetch_add(int_value(delta).unwrap_or(0), Ordering::SeqCst))
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_type_id(val: Value) -> i64 {
    value::type_id_raw(val) as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_value_eq(a: Value, b: Value) -> Value {
    Value::from_bool(value::value_eq(a, b))
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_identity_eq(a: Value, b: Value) -> Value {
    let ok = a.0 == b.0 && !(a.is_float() && b.is_float() && a.as_float().is_nan());
    Value::from_bool(ok)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_str_from_utf8(ptr: *const u8, len: usize) -> Value {
    string::str_from_utf8(ptr, len)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_str_intern(val: Value) -> Value {
    string::str_intern(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_str_concat(parts_ptr: *const Value, parts_len: usize) -> Value {
    string::str_concat(parts_ptr, parts_len)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_str_concat_local(parts_ptr: *const Value, parts_len: usize) -> Value {
    string::str_concat_local(parts_ptr, parts_len)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_bytes_from_string(val: Value) -> Value {
    bytes::bytes_from_string(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_bytes_from_slice_local(ptr: *const u8, len: usize) -> Value {
    if ptr.is_null() && len != 0 {
        return Value::nil();
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    bytes::bytes_from_slice_local(bytes)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_bytes_to_string(val: Value) -> Value {
    bytes::bytes_to_string(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_bytes_len(val: Value) -> Value {
    bytes::bytes_len(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_bytes_to_list(val: Value) -> Value {
    bytes::bytes_to_list(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_bytes_from_list(val: Value) -> Value {
    bytes::bytes_from_list(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_list_new(len: usize) -> Value {
    list::list_new(len)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_list_new_local(len: usize) -> Value {
    list::list_new_local(len)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_list_get(list_val: Value, idx: usize) -> Value {
    list::list_get(list_val, idx)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_list_set(list_val: Value, idx: usize, val: Value) {
    list::list_set(list_val, idx, val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_list_push(list_val: Value, val: Value) {
    list::list_push(list_val, val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_list_len(list_val: Value) -> Value {
    list::list_len(list_val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_num_add(a: Value, b: Value) -> Value {
    num_add(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_num_sub(a: Value, b: Value) -> Value {
    num_sub(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_num_mul(a: Value, b: Value) -> Value {
    num_mul(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_num_div(a: Value, b: Value) -> Value {
    num_div(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_num_mod(a: Value, b: Value) -> Value {
    num_mod(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_num_neg(a: Value) -> Value {
    num_neg(a)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_num_lt(a: Value, b: Value) -> Value {
    num_lt(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_num_gt(a: Value, b: Value) -> Value {
    num_gt(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_num_le(a: Value, b: Value) -> Value {
    num_le(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_num_ge(a: Value, b: Value) -> Value {
    num_ge(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_map_new() -> Value {
    map::map_new()
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_map_new_local() -> Value {
    map::map_new_local()
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_map_get(map_val: Value, key: Value) -> Value {
    map::map_get(map_val, key)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_map_set(map_val: Value, key: Value, val: Value) -> Value {
    map::map_set(map_val, key, val);
    Value::nil()
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_print(val: Value) -> Value {
    if val.is_ptr() {
        unsafe {
            let header = &*val.as_ptr();
            if header.type_id == TypeId::String as u32 {
                let _ = string::with_string_bytes(val, |bytes| {
                    println!("{}", String::from_utf8_lossy(bytes));
                });
                return Value::nil();
            }
        }
    }
    println!("<value>");
    Value::nil()
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_log(level: Value, msg: Value, fields: Value) -> Value {
    logging::log(level, msg, fields)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_log_configure(config: Value) -> Value {
    logging::log_configure(config)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_assert(cond: Value, msg: Value) -> Value {
    let ok = if cond.is_bool() {
        cond.as_bool()
    } else {
        false
    };
    if ok {
        return Value::nil();
    }
    if msg.is_ptr() {
        unsafe {
            let header = &*msg.as_ptr();
            if header.type_id == TypeId::String as u32 {
                let _ = string::with_string_bytes(msg, |bytes| {
                    eprintln!("assert: {}", String::from_utf8_lossy(bytes));
                });
                diagnostics::dump_diagnostics();
                std::process::abort();
            }
        }
    }
    eprintln!("assert failed");
    diagnostics::dump_diagnostics();
    std::process::abort();
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_assert_eq(left: Value, right: Value) -> Value {
    if value::value_eq(left, right) {
        return Value::nil();
    }
    eprintln!("assert_eq failed");
    diagnostics::dump_diagnostics();
    std::process::abort();
}

fn deep_eq(a: Value, b: Value, depth: usize, seen: &mut HashSet<(usize, usize)>) -> bool {
    if depth > 16 {
        return false;
    }
    if value::value_eq(a, b) {
        return true;
    }
    if a.is_ptr() && b.is_ptr() {
        let ap = a.as_ptr() as usize;
        let bp = b.as_ptr() as usize;
        let key = if ap <= bp { (ap, bp) } else { (bp, ap) };
        if !seen.insert(key) {
            return true;
        }
        unsafe {
            let ah = &*a.as_ptr();
            let bh = &*b.as_ptr();
            if ah.type_id != bh.type_id {
                return false;
            }
            if ah.type_id == TypeId::List as u32 {
                let Some(al) = crate::list::as_list_ref(a) else {
                    return false;
                };
                let Some(bl) = crate::list::as_list_ref(b) else {
                    return false;
                };
                if (*al).len != (*bl).len {
                    return false;
                }
                for idx in 0..(*al).len {
                    let av = (&(*al).data)[idx];
                    let bv = (&(*bl).data)[idx];
                    if !deep_eq(av, bv, depth + 1, seen) {
                        return false;
                    }
                }
                return true;
            }
            if ah.type_id == TypeId::Map as u32 {
                let Some(am) = crate::map::as_map_ref(a) else {
                    return false;
                };
                let Some(bm) = crate::map::as_map_ref(b) else {
                    return false;
                };
                if crate::map::map_len(am) != crate::map::map_len(bm) {
                    return false;
                }
                let mut iter = crate::map::map_iter(am);
                while let Some((key, val)) = iter.next() {
                    let Some(other) = crate::map::map_get_raw(bm, key) else {
                        return false;
                    };
                    if !deep_eq(val, other, depth + 1, seen) {
                        return false;
                    }
                }
                return true;
            }
            if ah.type_id == TypeId::Result as u32 {
                let Some((a_ok, a_val)) = result::result_parts(a) else {
                    return false;
                };
                let Some((b_ok, b_val)) = result::result_parts(b) else {
                    return false;
                };
                if a_ok != b_ok {
                    return false;
                }
                return deep_eq(a_val, b_val, depth + 1, seen);
            }
        }
    }
    false
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_value_deep_eq(left: Value, right: Value) -> Value {
    let mut seen: HashSet<(usize, usize)> = HashSet::new();
    Value::from_bool(deep_eq(left, right, 0, &mut seen))
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_assert_value_equality(left: Value, right: Value) -> Value {
    let mut seen: HashSet<(usize, usize)> = HashSet::new();
    if deep_eq(left, right, 0, &mut seen) {
        return Value::nil();
    }
    eprintln!("assert_value_equality failed");
    diagnostics::dump_diagnostics();
    std::process::abort();
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_assert_identity(left: Value, right: Value) -> Value {
    if left.0 == right.0 {
        if left.is_float() && right.is_float() && left.as_float().is_nan() {
            eprintln!("assert_identity failed");
            diagnostics::dump_diagnostics();
            std::process::abort();
        }
        return Value::nil();
    }
    eprintln!("assert_identity failed");
    diagnostics::dump_diagnostics();
    std::process::abort();
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_assert_err(val: Value) -> Value {
    if val.is_ptr() {
        unsafe {
            let header = &*val.as_ptr();
            if header.type_id == TypeId::Result as u32 {
                let ok = result::result_is_ok(val);
                if ok.is_bool() && !ok.as_bool() {
                    return Value::nil();
                }
            }
        }
    }
    eprintln!("assert_err failed");
    diagnostics::dump_diagnostics();
    std::process::abort();
}

fn builtin_error(message: &str) -> Value {
    string::str_from_utf8(message.as_ptr(), message.len())
}

fn string_bytes(val: Value) -> Option<Vec<u8>> {
    string::with_string_bytes(val, |bytes| bytes.to_vec())
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_fs_read_bytes(path: Value) -> Value {
    let Some(bytes) = string_bytes(path) else {
        return result::result_err(builtin_error("fs_read_bytes expects a String"));
    };
    let path_str = String::from_utf8_lossy(&bytes);
    match std::fs::read(path_str.as_ref()) {
        Ok(contents) => result::result_ok(bytes::bytes_from_slice(&contents)),
        Err(err) => result::result_err(builtin_error(&format!("fs_read_bytes: {err}"))),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_fs_write_bytes(path: Value, contents: Value) -> Value {
    let Some(path_bytes) = string_bytes(path) else {
        return result::result_err(builtin_error("fs_write_bytes expects a String path"));
    };
    let Some(contents_bytes) = bytes::with_bytes(contents, |bytes| bytes.to_vec()) else {
        return result::result_err(builtin_error("fs_write_bytes expects Bytes contents"));
    };
    let path_str = String::from_utf8_lossy(&path_bytes);
    match std::fs::write(path_str.as_ref(), contents_bytes) {
        Ok(()) => result::result_ok(Value::nil()),
        Err(err) => result::result_err(builtin_error(&format!("fs_write_bytes: {err}"))),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_result_ok(val: Value) -> Value {
    result::result_ok(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_result_err(val: Value) -> Value {
    result::result_err(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_result_is_ok(val: Value) -> Value {
    result::result_is_ok(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_result_unwrap(val: Value) -> Value {
    result::result_unwrap(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_result_err_unwrap(val: Value) -> Value {
    result::result_err_unwrap(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_crash(val: Value) -> Value {
    if val.is_ptr() {
        unsafe {
            let header = &*val.as_ptr();
            if header.type_id == TypeId::String as u32 {
                let _ = string::with_string_bytes(val, |bytes| {
                    eprintln!("crash: {}", String::from_utf8_lossy(bytes));
                });
                diagnostics::dump_diagnostics();
                std::process::abort();
            }
        }
    }
    let tid = wr_type_id(val);
    eprintln!("crash (type_id={tid})");
    diagnostics::dump_diagnostics();
    std::process::abort();
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_actor_spawn(
    class_id: u64,
    instance: Value,
    pool_size: i64,
    objective: i64,
    mailbox_cap: i64,
    enqueue_timeout_ms: i64,
    batch_limit: i64,
) -> Value {
    actor::actor_spawn(
        class_id,
        instance,
        pool_size,
        objective,
        mailbox_cap,
        enqueue_timeout_ms,
        batch_limit,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_pool_new(
    handles: Value,
    objective: i64,
    min_size: i64,
    max_size: i64,
    weight: i64,
    queue_cap: i64,
) -> Value {
    actor::pool_new(handles, objective, min_size, max_size, weight, queue_cap)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_runtime_cpu_count() -> Value {
    let count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1) as i64;
    Value::from_int(count)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_pool_size(handle: Value) -> Value {
    actor::pool_size(handle)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_pool_rr(handle: Value) -> Value {
    actor::pool_rr(handle)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_pool_queue_len(handle: Value) -> Value {
    actor::pool_queue_len(handle)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_actor_mailbox_len(handle: Value) -> Value {
    actor::actor_mailbox_len(handle)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_actor_pause(handle: Value) -> Value {
    actor::actor_pause(handle);
    Value::nil()
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_actor_resume(handle: Value) -> Value {
    actor::actor_resume(handle);
    Value::nil()
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_actor_pause_wait(handle: Value) -> Value {
    actor::actor_pause_wait(handle);
    Value::nil()
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_sleep_ms(ms_val: Value) -> Value {
    let ms = int_value(ms_val).unwrap_or(0);
    actor::sleep_ms(ms)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_metrics_get(id_val: Value) -> Value {
    let id = int_value(id_val).unwrap_or(0) as u32;
    Value::from_int(metrics::get(id) as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_metrics_dropped_paused_id() -> Value {
    Value::from_int(metrics::METRIC_MESSAGES_DROPPED_PAUSED as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_metrics_messages_dropped_id() -> Value {
    Value::from_int(metrics::METRIC_MESSAGES_DROPPED as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_clock_ns() -> Value {
    static START: OnceLock<Instant> = OnceLock::new();
    let start = START.get_or_init(Instant::now);
    let ns = start.elapsed().as_nanos() as i64;
    Value::from_int(ns)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_list_push_val(list_val: Value, val: Value) -> Value {
    list::list_push(list_val, val);
    Value::nil()
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_actor_send(
    handle: Value,
    method_id: u32,
    argc: usize,
    argv_ptr: *const Value,
) -> Value {
    actor::actor_send(handle, method_id, argc, argv_ptr)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_actor_fire(
    handle: Value,
    method_id: u32,
    argc: usize,
    argv_ptr: *const Value,
) {
    actor::actor_fire(handle, method_id, argc, argv_ptr)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_pending_await(pending: Value) -> Value {
    actor::pending_await(pending)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_register_method(class_id: u32, method_id: u32, func: actor::MethodFn) {
    actor::register_method(class_id, method_id, func)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_register_class(name_ptr: *const u8, len: usize, class_id: u32) {
    let _ = (name_ptr, len, class_id);
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_register_method_name(
    name_ptr: *const u8,
    len: usize,
    class_id: u32,
    method_id: u32,
) {
    let _ = (name_ptr, len, class_id, method_id);
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_iter_init(iterable: Value) -> Value {
    iter::iter_init(iterable)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_iter_next(iter_val: Value, dst_value: *mut Value, dst_done: *mut Value) {
    iter::iter_next(iter_val, dst_value, dst_done)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_class_new(
    class_id: u32,
    names_ptr: *const *const u8,
    lens_ptr: *const usize,
    count: usize,
) -> Value {
    class::class_new(class_id, names_ptr, lens_ptr, count)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_class_get(obj: Value, name_ptr: *const u8, len: usize) -> Value {
    class::class_get(obj, name_ptr, len)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_class_set(obj: Value, name_ptr: *const u8, len: usize, val: Value) {
    class::class_set(obj, name_ptr, len, val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_range_new(start: Value, end: Value) -> Value {
    range_new(start, end)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_metrics_reset() {
    metrics::reset()
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_runtime_configure(config: Value) -> Value {
    config::runtime_configure(config)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_env_get(key: Value) -> Value {
    env_get(key)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_env_set(key: Value, val: Value) -> Value {
    env_set(key, val)
}

fn value_to_string(val: Value) -> Option<String> {
    string::with_string_bytes(val, |bytes| String::from_utf8_lossy(bytes).into_owned())
}

fn env_get(key: Value) -> Value {
    let key = match value_to_string(key) {
        Some(key) => key,
        None => return Value::nil(),
    };
    match std::env::var(&key).ok() {
        Some(val) => string::str_from_bytes(val.as_bytes()),
        None => Value::nil(),
    }
}

fn env_set(key: Value, val: Value) -> Value {
    let key = match value_to_string(key) {
        Some(key) => key,
        None => return Value::from_bool(false),
    };
    let val = match value_to_string(val) {
        Some(val) => val,
        None => return Value::from_bool(false),
    };
    unsafe {
        std::env::set_var(key, val);
    }
    Value::from_bool(true)
}

fn num_add(a: Value, b: Value) -> Value {
    if is_string(a) && is_string(b) {
        let parts = [a, b];
        return string::str_concat(parts.as_ptr(), parts.len());
    }
    numeric_binary(a, b, |x, y| x + y, |x, y| x + y)
}

fn num_sub(a: Value, b: Value) -> Value {
    numeric_binary(a, b, |x, y| x - y, |x, y| x - y)
}

fn num_mul(a: Value, b: Value) -> Value {
    numeric_binary(a, b, |x, y| x * y, |x, y| x * y)
}

fn num_div(a: Value, b: Value) -> Value {
    match (num_kind(a), num_kind(b)) {
        (Some(NumKind::Integer(x)), Some(NumKind::Integer(y))) => {
            if y == 0 {
                std::process::abort();
            }
            Value::from_int(x / y)
        }
        (Some(x), Some(y)) => {
            let xf = num_to_f64(x);
            let yf = num_to_f64(y);
            Value::from_float(xf / yf)
        }
        _ => Value::nil(),
    }
}

fn num_mod(a: Value, b: Value) -> Value {
    match (num_kind(a), num_kind(b)) {
        (Some(NumKind::Integer(x)), Some(NumKind::Integer(y))) => {
            if y == 0 {
                std::process::abort();
            }
            Value::from_int(x % y)
        }
        (Some(x), Some(y)) => {
            let xf = num_to_f64(x);
            let yf = num_to_f64(y);
            Value::from_float(xf % yf)
        }
        _ => Value::nil(),
    }
}

fn num_neg(a: Value) -> Value {
    match num_kind(a) {
        Some(NumKind::Integer(x)) => Value::from_int(-x),
        Some(NumKind::Float(x)) => Value::from_float(-x),
        None => Value::nil(),
    }
}

fn num_lt(a: Value, b: Value) -> Value {
    Value::from_bool(numeric_cmp(a, b, |x, y| x < y, |x, y| x < y))
}

fn num_gt(a: Value, b: Value) -> Value {
    Value::from_bool(numeric_cmp(a, b, |x, y| x > y, |x, y| x > y))
}

fn num_le(a: Value, b: Value) -> Value {
    Value::from_bool(numeric_cmp(a, b, |x, y| x <= y, |x, y| x <= y))
}

fn num_ge(a: Value, b: Value) -> Value {
    Value::from_bool(numeric_cmp(a, b, |x, y| x >= y, |x, y| x >= y))
}

fn range_new(start: Value, end: Value) -> Value {
    match (num_kind(start), num_kind(end)) {
        (Some(NumKind::Integer(a)), Some(NumKind::Integer(b))) => range_int(a, b),
        (Some(a), Some(b)) => range_float(num_to_f64(a), num_to_f64(b)),
        _ => list::list_new(0),
    }
}

fn range_int(start: i64, end: i64) -> Value {
    let list_val = list::list_new(0);
    let step = if start <= end { 1 } else { -1 };
    let mut current = start;
    loop {
        list::list_push(list_val, Value::from_int(current));
        if current == end {
            break;
        }
        current = current.saturating_add(step);
    }
    list_val
}

fn range_float(start: f64, end: f64) -> Value {
    if !start.is_finite() || !end.is_finite() {
        return list::list_new(0);
    }
    let list_val = list::list_new(0);
    let step = if start <= end { 1.0 } else { -1.0 };
    let mut current = start;
    loop {
        list::list_push(list_val, Value::from_float(current));
        if (step > 0.0 && current >= end) || (step < 0.0 && current <= end) {
            break;
        }
        current += step;
        if !current.is_finite() {
            break;
        }
    }
    list_val
}

fn numeric_binary(
    a: Value,
    b: Value,
    int_op: impl FnOnce(i64, i64) -> i64,
    float_op: impl FnOnce(f64, f64) -> f64,
) -> Value {
    match (num_kind(a), num_kind(b)) {
        (Some(NumKind::Integer(x)), Some(NumKind::Integer(y))) => Value::from_int(int_op(x, y)),
        (Some(x), Some(y)) => {
            let xf = num_to_f64(x);
            let yf = num_to_f64(y);
            Value::from_float(float_op(xf, yf))
        }
        _ => Value::nil(),
    }
}

fn numeric_cmp(
    a: Value,
    b: Value,
    int_op: impl FnOnce(i64, i64) -> bool,
    float_op: impl FnOnce(f64, f64) -> bool,
) -> bool {
    match (num_kind(a), num_kind(b)) {
        (Some(NumKind::Integer(x)), Some(NumKind::Integer(y))) => int_op(x, y),
        (Some(x), Some(y)) => {
            let xf = num_to_f64(x);
            let yf = num_to_f64(y);
            float_op(xf, yf)
        }
        _ => false,
    }
}

fn num_to_f64(kind: NumKind) -> f64 {
    match kind {
        NumKind::Integer(x) => x as f64,
        NumKind::Float(x) => x,
    }
}

fn num_kind(val: Value) -> Option<NumKind> {
    if let Some(i) = int_value(val) {
        return Some(NumKind::Integer(i));
    }
    if val.is_float() {
        return Some(NumKind::Float(val.as_float()));
    }
    None
}

fn is_string(val: Value) -> bool {
    if !val.is_ptr() {
        return false;
    }
    unsafe { (*val.as_ptr()).type_id == TypeId::String as u32 }
}

enum NumKind {
    Integer(i64),
    Float(f64),
}

#[cfg(test)]
mod tests;
