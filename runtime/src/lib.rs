#![allow(clippy::missing_safety_doc)]

mod data;
pub mod db;
mod host;
mod kernel;
pub mod reactor;
mod unsafe_primitives;

pub(crate) use data::{arena, bytes, class, iter, list, map, object, result, string, value};
pub(crate) use kernel::{actor, config, diagnostics, metrics, scheduler};

use data::object::drop_object;
use data::value::int_value;
pub use data::value::{TypeId, Value};
use std::collections::{HashMap, HashSet};
#[cfg(test)]
use std::sync::atomic::AtomicU8;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

const WR_REACTOR_EVENT_READABLE: i32 = 1;
const WR_REACTOR_EVENT_TIMER: i32 = 2;
#[cfg(test)]
const ABI_TYPED_LANE_UNKNOWN: u8 = 0;
#[cfg(all(test, feature = "abi_typed_fast_path"))]
const ABI_TYPED_LANE_ENABLED: u8 = 1;
#[cfg(all(test, feature = "abi_typed_fast_path"))]
const ABI_TYPED_LANE_DISABLED: u8 = 2;

#[cfg(test)]
static ABI_TYPED_LANE_CACHE: AtomicU8 = AtomicU8::new(ABI_TYPED_LANE_UNKNOWN);

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
    if arena::is_arena_ptr(value.as_ptr()) {
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
    if arena::is_arena_ptr(value.as_ptr()) {
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

fn db_value_to_bytes(value: Value) -> Option<Vec<u8>> {
    crate::string::with_string_bytes(value, |bytes| bytes.to_vec())
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_db_open(path: Value) -> Value {
    let Some(path_bytes) = db_value_to_bytes(path) else {
        return Value::nil();
    };
    let Ok(path_str) = std::str::from_utf8(&path_bytes) else {
        return Value::nil();
    };
    match db::open_db(std::path::Path::new(path_str)) {
        Ok(handle) => Value::from_int(handle),
        Err(_) => Value::nil(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_db_close(handle: Value) -> Value {
    Value::from_bool(db::close_db(int_value(handle).unwrap_or(0)))
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_db_submit_batch(
    handle: Value,
    namespace: Value,
    key: Value,
    value: Value,
    expected_version: Value,
) -> Value {
    let handle = int_value(handle).unwrap_or(0);
    let Some(namespace) = db_value_to_bytes(namespace) else {
        return Value::nil();
    };
    let Some(key) = db_value_to_bytes(key) else {
        return Value::nil();
    };
    let Some(value) = db_value_to_bytes(value) else {
        return Value::nil();
    };
    let expected_version = if expected_version.is_nil() {
        None
    } else {
        int_value(expected_version).map(|v| v.max(0) as u64)
    };
    let scratch_min = namespace
        .len()
        .saturating_add(key.len())
        .saturating_add(value.len())
        .saturating_add(32);
    db::abi::buffers::with_scratch(scratch_min, |scratch| {
        let frame = db::codec::BatchPutView {
            namespace: &namespace,
            key: &key,
            value: &value,
            expected_version,
        };
        if db::codec::encode_single_put_frame_into(frame, scratch).is_err() {
            return Value::nil();
        }
        let Ok(decoded) = db::codec::decode_single_put_frame(scratch.as_slice()) else {
            return Value::nil();
        };
        match db::submit_put(
            handle,
            decoded.namespace.to_vec(),
            decoded.key.to_vec(),
            decoded.value.to_vec(),
            decoded.expected_version,
        ) {
            Ok(version) => Value::from_int(version as i64),
            Err(_) => Value::nil(),
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_db_read_point(handle: Value, namespace: Value, key: Value) -> Value {
    let handle = int_value(handle).unwrap_or(0);
    let Some(namespace) = db_value_to_bytes(namespace) else {
        return Value::nil();
    };
    let Some(key) = db_value_to_bytes(key) else {
        return Value::nil();
    };
    match db::read_point(handle, namespace, key) {
        Ok(Some(bytes)) => match db::codec::decode_value_legacy_aware(&bytes) {
            Ok(payload) => crate::bytes::bytes_from_slice_local(payload),
            Err(_) => Value::nil(),
        },
        Ok(None) => Value::nil(),
        Err(_) => Value::nil(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_db_read_range(
    handle: Value,
    namespace: Value,
    start_key: Value,
    end_key: Value,
    limit: Value,
) -> Value {
    let handle = int_value(handle).unwrap_or(0);
    let Some(namespace) = db_value_to_bytes(namespace) else {
        return Value::nil();
    };
    let Some(start_key) = db_value_to_bytes(start_key) else {
        return Value::nil();
    };
    let Some(end_key) = db_value_to_bytes(end_key) else {
        return Value::nil();
    };
    let limit = int_value(limit).unwrap_or(100).max(1) as usize;
    match db::read_range(handle, namespace, start_key, end_key, limit) {
        Ok(rows) => {
            let out = crate::list::list_new(rows.len());
            for (_, value, _) in rows {
                let decoded = db::codec::decode_value_legacy_aware(&value).unwrap_or(&value);
                crate::list::list_push(out, crate::bytes::bytes_from_slice_local(decoded));
            }
            out
        }
        Err(_) => Value::nil(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_db_txn_begin(_handle: Value) -> Value {
    let handle = int_value(_handle).unwrap_or(0);
    match db::txn_begin(handle) {
        Ok(txn) => Value::from_int(txn as i64),
        Err(_) => Value::nil(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_db_txn_prepare(handle: Value, txn: Value) -> Value {
    let handle = int_value(handle).unwrap_or(0);
    let Some(txn) = int_value(txn).filter(|v| *v > 0).map(|v| v as u64) else {
        return Value::from_bool(false);
    };
    Value::from_bool(db::txn_prepare(handle, txn).is_ok())
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_db_txn_commit(handle: Value, txn: Value) -> Value {
    let handle = int_value(handle).unwrap_or(0);
    let Some(txn) = int_value(txn).filter(|v| *v > 0).map(|v| v as u64) else {
        return Value::from_bool(false);
    };
    Value::from_bool(db::txn_commit(handle, txn).is_ok())
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_db_txn_abort(handle: Value, txn: Value) -> Value {
    let handle = int_value(handle).unwrap_or(0);
    let Some(txn) = int_value(txn).filter(|v| *v > 0).map(|v| v as u64) else {
        return Value::from_bool(false);
    };
    Value::from_bool(db::txn_abort(handle, txn).is_ok())
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_db_snapshot_start(handle: Value) -> Value {
    let handle = int_value(handle).unwrap_or(0);
    match db::snapshot_start(handle) {
        Ok(snapshot_id) => Value::from_int(snapshot_id as i64),
        Err(_) => Value::nil(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_db_snapshot_status(handle: Value, snapshot: Value) -> Value {
    let handle = int_value(handle).unwrap_or(0);
    let Some(snapshot) = int_value(snapshot).filter(|v| *v > 0).map(|v| v as u64) else {
        return Value::nil();
    };
    match db::snapshot_status(handle, snapshot) {
        Ok(progress) => Value::from_int(progress as i64),
        Err(_) => Value::nil(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_db_restore(handle: Value, snapshot: Value) -> Value {
    let handle = int_value(handle).unwrap_or(0);
    let Some(snapshot) = int_value(snapshot).filter(|v| *v > 0).map(|v| v as u64) else {
        return Value::from_bool(false);
    };
    Value::from_bool(db::restore_snapshot(handle, snapshot).is_ok())
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_runtime_caps() -> u64 {
    unsafe_primitives::runtime_caps_mask()
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
pub extern "C" fn wr_str_intern_utf8(ptr: *const u8, len: usize) -> Value {
    string::str_intern_utf8(ptr, len)
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
pub extern "C" fn wr_str_len(val: Value) -> Value {
    if let Some(len) = crate::string::with_string_bytes(val, |b| b.len()) {
        Value::from_int(len as i64)
    } else {
        Value::nil()
    }
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
pub extern "C" fn wr_map_len(map_val: Value) -> Value {
    let Some(map) = map::as_map_ref(map_val) else {
        return Value::nil();
    };
    Value::from_int(map::map_len(map) as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_map_set(map_val: Value, key: Value, val: Value) -> Value {
    map::map_set(map_val, key, val);
    Value::nil()
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_print(val: Value) -> Value {
    host::print(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_log(level: Value, msg: Value, fields: Value) -> Value {
    host::log(level, msg, fields)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_log_configure(config: Value) -> Value {
    host::log_configure(config)
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

#[unsafe(no_mangle)]
pub extern "C" fn wr_fs_read_bytes(path: Value) -> Value {
    host::fs_read_bytes(path)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_fs_write_bytes(path: Value, contents: Value) -> Value {
    host::fs_write_bytes(path, contents)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_external_call(
    service: Value,
    endpoint: Value,
    method: Value,
    url: Value,
    headers: Value,
    body: Value,
    timeout_ms: Value,
) -> Value {
    host::external_call(service, endpoint, method, url, headers, body, timeout_ms)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_http_call(
    service: Value,
    endpoint: Value,
    method: Value,
    url: Value,
    headers: Value,
    body: Value,
    timeout_ms: Value,
) -> Value {
    host::http_call(service, endpoint, method, url, headers, body, timeout_ms)
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
    host::sleep_ms(ms_val)
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
    host::clock_ns()
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
pub extern "C" fn wr_actor_fire_burst_begin(handle: Value) -> Value {
    actor::actor_fire_burst_begin(handle);
    Value::nil()
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_actor_fire_burst_end(handle: Value) -> Value {
    actor::actor_fire_burst_end(handle);
    Value::nil()
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_actor_fire_burst_abort(handle: Value) -> Value {
    actor::actor_fire_burst_abort(handle);
    Value::nil()
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
    let obj = crate::kernel::actor::actor_backing_instance(obj).unwrap_or(obj);
    class::class_get(obj, name_ptr, len)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_class_get_slot(
    obj: Value,
    name_ptr: *const u8,
    len: usize,
    slot: usize,
) -> Value {
    let obj = crate::kernel::actor::actor_backing_instance(obj).unwrap_or(obj);
    class::class_get_slot(obj, name_ptr, len, slot as u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_class_set(obj: Value, name_ptr: *const u8, len: usize, val: Value) {
    let obj = crate::kernel::actor::actor_backing_instance(obj).unwrap_or(obj);
    class::class_set(obj, name_ptr, len, val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_class_set_slot(
    obj: Value,
    name_ptr: *const u8,
    len: usize,
    slot: usize,
    val: Value,
) {
    let obj = crate::kernel::actor::actor_backing_instance(obj).unwrap_or(obj);
    class::class_set_slot(obj, name_ptr, len, slot as u32, val)
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
pub extern "C" fn wr_coverage_hit(function_id: i64) -> i64 {
    metrics::coverage_hit(function_id as u64);
    function_id
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_runtime_configure(config: Value) -> Value {
    config::runtime_configure(config)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_env_get(key: Value) -> Value {
    host::env_get(key)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_env_set(key: Value, val: Value) -> Value {
    host::env_set(key, val)
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

#[cfg(all(test, feature = "abi_typed_fast_path"))]
fn abi_flag_truthy(name: &str) -> bool {
    let Some(raw) = std::env::var_os(name) else {
        return false;
    };
    let lower = raw.to_string_lossy().to_ascii_lowercase();
    matches!(lower.as_str(), "1" | "true" | "on" | "yes")
}

#[cfg(test)]
fn abi_typed_lane_enabled() -> bool {
    #[cfg(feature = "abi_typed_fast_path")]
    {
        match ABI_TYPED_LANE_CACHE.load(Ordering::Relaxed) {
            ABI_TYPED_LANE_ENABLED => return true,
            ABI_TYPED_LANE_DISABLED => return false,
            _ => {}
        }

        let enabled = abi_flag_truthy("WRELA_ABI_TYPED_FAST_PATH");
        ABI_TYPED_LANE_CACHE.store(
            if enabled {
                ABI_TYPED_LANE_ENABLED
            } else {
                ABI_TYPED_LANE_DISABLED
            },
            Ordering::Relaxed,
        );
        enabled
    }
    #[cfg(not(feature = "abi_typed_fast_path"))]
    {
        false
    }
}

#[cfg(test)]
fn abi_refresh_typed_lane_cache() {
    ABI_TYPED_LANE_CACHE.store(ABI_TYPED_LANE_UNKNOWN, Ordering::Relaxed);
}

#[cfg(test)]
fn abi_roundtrip_i64(val: i64) -> i64 {
    if abi_typed_lane_enabled() {
        metrics::inc_abi_typed_lane();
        return val;
    }
    metrics::inc_abi_boxed_lane();
    let boxed = value::force_boxed_int(val);
    let out = int_value(boxed).unwrap_or(0);
    unsafe {
        wr_rc_dec(boxed);
    }
    out
}

#[cfg(test)]
fn abi_roundtrip_value(val: Value) -> Value {
    let input = int_value(val).unwrap_or(0);
    Value::from_int(abi_roundtrip_i64(input))
}

#[cfg(test)]
mod tests {
    use crate::*;
    use sha2::Digest;
    use std::hint::black_box;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};
    use std::time::Instant;

    fn abi_test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn str_value(input: &str) -> Value {
        wr_str_from_utf8(input.as_ptr(), input.len())
    }

    fn value_to_string(input: Value) -> String {
        crate::string::with_string_bytes(input, |bytes| String::from_utf8_lossy(bytes).to_string())
            .unwrap_or_default()
    }

    fn dec(input: Value) {
        unsafe {
            wr_rc_dec(input);
        }
    }

    fn temp_db_dir() -> PathBuf {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let dir = std::env::temp_dir().join(format!(
            "wrela_runtime_db_{}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("unix epoch")
                .as_nanos(),
            NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("create temp db dir");
        dir
    }

    #[test]
    fn boxing_round_trip() {
        let int = wr_box_int(42);
        assert_eq!(wr_unbox_int(int), 42);

        let float = wr_box_float(3.5);
        assert_eq!(wr_unbox_float(float), 3.5);
    }

    #[test]
    fn abi_roundtrip_boxed_lane() {
        let _guard = abi_test_lock().lock().expect("abi test lock");
        let _metrics_guard = metrics::test_lock().lock().expect("metrics test lock");
        unsafe {
            std::env::remove_var("WRELA_ABI_TYPED_FAST_PATH");
        }
        abi_refresh_typed_lane_cache();
        metrics::reset();

        assert_eq!(abi_roundtrip_i64(42), 42);
        let value = abi_roundtrip_value(Value::from_int(7));
        assert_eq!(value.as_int(), 7);
    }

    #[cfg(feature = "abi_typed_fast_path")]
    #[test]
    fn abi_roundtrip_typed_lane() {
        let _guard = abi_test_lock().lock().expect("abi test lock");
        let _metrics_guard = metrics::test_lock().lock().expect("metrics test lock");
        unsafe {
            std::env::set_var("WRELA_ABI_TYPED_FAST_PATH", "1");
        }
        abi_refresh_typed_lane_cache();
        metrics::reset();

        assert_eq!(abi_roundtrip_i64(123), 123);
        let value = abi_roundtrip_value(Value::from_int(-11));
        assert_eq!(value.as_int(), -11);
    }

    #[test]
    fn string_and_bytes_round_trip() {
        let hello = str_value("hello");
        let world = str_value(" world");
        let parts = [hello, world];

        let joined = wr_str_concat(parts.as_ptr(), parts.len());
        assert_eq!(value_to_string(joined), "hello world");

        let bytes = wr_bytes_from_string(joined);
        let len = wr_bytes_len(bytes);
        assert_eq!(len.as_int(), 11);

        let decoded = wr_bytes_to_string(bytes);
        assert_eq!(value_to_string(decoded), "hello world");

        dec(hello);
        dec(world);
        dec(joined);
        dec(bytes);
        dec(decoded);
    }

    #[test]
    fn list_and_map_ops() {
        let list = wr_list_new(0);
        let one = wr_box_int(1);
        let two = wr_box_int(2);

        wr_list_push(list, one);
        wr_list_push(list, two);

        assert_eq!(wr_list_len(list).as_int(), 2);
        assert_eq!(wr_list_get(list, 1).as_int(), 2);

        let map = wr_map_new();
        let key = str_value("k");
        let val = str_value("v");
        let _ = wr_map_set(map, key, val);
        let got = wr_map_get(map, key);

        assert_eq!(value_to_string(got), "v");

        dec(list);
        dec(one);
        dec(two);
        dec(map);
        dec(key);
        dec(val);
        dec(got);
    }

    #[test]
    fn map_inline_cache_hits_and_invalidation_fallback_correctness() {
        crate::map::map_ic_reset_stats();
        let map = wr_map_new();
        let key = str_value("k");
        let miss_before_set = wr_map_get(map, key);
        assert!(miss_before_set.is_nil());
        dec(miss_before_set);

        let value = Value::from_int(9);
        let _ = wr_map_set(map, key, value);

        let after_set = wr_map_get(map, key);
        assert_eq!(after_set.as_int(), 9);
        dec(after_set);

        let hot = wr_map_get(map, key);
        assert_eq!(hot.as_int(), 9);
        dec(hot);

        let (hits, misses) = crate::map::map_ic_stats();
        assert!(
            hits >= 1,
            "expected at least one cache hit after warm lookup, hits={hits}"
        );
        assert!(
            misses >= 2,
            "expected cold + invalidated miss path at least twice, misses={misses}"
        );

        dec(map);
        dec(key);
    }

    #[test]
    fn result_ops() {
        let ok = wr_result_ok(wr_box_int(7));
        assert!(wr_result_is_ok(ok).as_bool());
        assert_eq!(wr_result_unwrap(ok).as_int(), 7);

        let err_msg = str_value("bad");
        let err = wr_result_err(err_msg);
        assert!(!wr_result_is_ok(err).as_bool());
        assert_eq!(value_to_string(wr_result_err_unwrap(err)), "bad");

        dec(ok);
        dec(err_msg);
        dec(err);
    }

    #[test]
    fn env_ops() {
        let key = str_value("WRELA_TEST_ENV");
        let val = str_value("ok");

        let _ = wr_env_set(key, val);
        let got = wr_env_get(key);
        assert_eq!(value_to_string(got), "ok");

        dec(key);
        dec(val);
        dec(got);
    }

    #[test]
    fn db_abi_put_get_scan_roundtrip() {
        let dir = temp_db_dir();
        let path = str_value(&dir.to_string_lossy());
        let handle = wr_db_open(path);
        assert!(handle.is_int());
        assert!(handle.as_int() > 0);

        let namespace = str_value("core");
        let key = str_value("k1");
        let value = str_value("v1");
        let version = wr_db_submit_batch(handle, namespace, key, value, Value::nil());
        if int_value(version).is_none() {
            let direct = crate::db::submit_put(
                handle.as_int(),
                b"core".to_vec(),
                b"k1".to_vec(),
                b"v1".to_vec(),
                None,
            )
            .expect("direct put fallback");
            assert!(direct > 0);
        } else {
            assert!(int_value(version).unwrap_or(0) > 0);
        }

        let got = wr_db_read_point(handle, namespace, key);
        let got_str = wr_bytes_to_string(got);
        assert_eq!(value_to_string(got_str), "v1");

        let scan = wr_db_read_range(
            handle,
            namespace,
            str_value("k0"),
            str_value("kz"),
            Value::from_int(10),
        );
        assert!(scan.is_ptr());
        assert!(wr_list_len(scan).as_int() >= 1);

        let closed = wr_db_close(handle);
        assert!(closed.is_bool());
        assert!(closed.as_bool());

        dec(path);
        dec(namespace);
        dec(key);
        dec(value);
        dec(version);
        dec(got);
        dec(got_str);
        dec(scan);
        dec(closed);
    }

    #[test]
    fn db_abi_txn_and_snapshot_paths_are_stateful() {
        let dir = temp_db_dir();
        let path = str_value(&dir.to_string_lossy());
        let handle = wr_db_open(path);
        assert!(handle.is_int());
        assert!(handle.as_int() > 0);

        let txn = wr_db_txn_begin(handle);
        assert!(txn.is_int());
        assert!(txn.as_int() > 0);
        let prepared = wr_db_txn_prepare(handle, txn);
        let committed = wr_db_txn_commit(handle, txn);
        assert!(prepared.is_bool() && prepared.as_bool());
        assert!(committed.is_bool() && committed.as_bool());

        let snapshot = wr_db_snapshot_start(handle);
        assert!(snapshot.is_int());
        assert!(snapshot.as_int() > 0);
        let progress = wr_db_snapshot_status(handle, snapshot);
        assert!(progress.is_int());
        assert_eq!(progress.as_int(), 100);
        let restored = wr_db_restore(handle, snapshot);
        assert!(restored.is_bool() && restored.as_bool());

        let closed = wr_db_close(handle);
        assert!(closed.is_bool() && closed.as_bool());

        dec(path);
        dec(txn);
        dec(prepared);
        dec(committed);
        dec(snapshot);
        dec(progress);
        dec(restored);
        dec(closed);
    }

    #[test]
    fn external_call_stub_is_deterministic() {
        let service = str_value("billing");
        let endpoint = str_value("charge");
        let method = str_value("POST");
        let url = str_value("https://api.example.test/charges");
        let body = str_value("amount=100");
        let headers = wr_map_new();
        let header_key = str_value("x-request-id");
        let header_val = str_value("abc");
        let _ = wr_map_set(headers, header_key, header_val);
        let timeout_ms = Value::from_int(2500);

        let first = wr_external_call(service, endpoint, method, url, headers, body, timeout_ms);
        let second = wr_external_call(service, endpoint, method, url, headers, body, timeout_ms);

        assert!(wr_result_is_ok(first).as_bool());
        assert!(wr_result_is_ok(second).as_bool());
        let first_text = wr_result_unwrap(first);
        let second_text = wr_result_unwrap(second);
        assert_eq!(
            value_to_string(first_text),
            "external.stub:service=billing;endpoint=charge;method=POST;url=https://api.example.test/charges;headers=1;body_len=10;timeout_ms=2500"
        );
        assert_eq!(value_to_string(first_text), value_to_string(second_text));

        dec(service);
        dec(endpoint);
        dec(method);
        dec(url);
        dec(body);
        dec(headers);
        dec(header_key);
        dec(header_val);
        dec(first);
        dec(second);
        dec(first_text);
        dec(second_text);
    }

    #[test]
    fn http_call_replay_missing_cassette_returns_teacher_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        unsafe {
            std::env::set_var("WRELA_HTTP_MODE", "replay");
            std::env::set_var("WRELA_CASSETTE_DIR", dir.path());
        }

        let service = str_value("billing");
        let endpoint = str_value("charge");
        let method = str_value("POST");
        let url = str_value("http://127.0.0.1:9/missing");
        let body = str_value("amount=100");
        let headers = wr_map_new();
        let timeout_ms = Value::from_int(500);

        let result = wr_http_call(service, endpoint, method, url, headers, body, timeout_ms);
        assert!(!wr_result_is_ok(result).as_bool());
        let err = wr_result_err_unwrap(result);
        let err_text = value_to_string(err);
        assert!(err_text.contains("cassette missing for replay mode"));
        assert!(err_text.contains("wrela test --record"));

        dec(service);
        dec(endpoint);
        dec(method);
        dec(url);
        dec(body);
        dec(headers);
        dec(result);
        dec(err);
    }

    #[test]
    fn http_call_replay_rejects_unknown_cassette_version() {
        let dir = tempfile::tempdir().expect("tempdir");
        let service_name = "billing";
        let endpoint_name = "charge";
        let method_name = "post";
        let url_value = "http://127.0.0.1:9/missing";
        let body_value = "";
        let body_hash = {
            let mut hasher = sha2::Sha256::new();
            sha2::Digest::update(&mut hasher, body_value.as_bytes());
            format!("{:x}", sha2::Digest::finalize(hasher))
        };
        let url_hash = {
            let mut hasher = sha2::Sha256::new();
            sha2::Digest::update(&mut hasher, url_value.as_bytes());
            format!("{:x}", sha2::Digest::finalize(hasher))
        };
        let headers_hash = {
            let mut hasher = sha2::Sha256::new();
            sha2::Digest::update(&mut hasher, b"");
            format!("{:x}", sha2::Digest::finalize(hasher))
        };
        let cassette = dir.path().join(format!(
            "{}__{}__{}__{}__{}__{}.json",
            service_name, endpoint_name, method_name, url_hash, body_hash, headers_hash
        ));
        std::fs::write(
            &cassette,
            r#"{
  "version": 99,
  "request": {
    "service": "billing",
    "endpoint": "charge",
    "method": "POST",
    "url": "http://127.0.0.1:9/missing",
    "headers_redacted": {},
    "body_base64": ""
  },
  "response": {
    "status": 200,
    "headers": {},
    "body_base64": ""
  }
}"#,
        )
        .expect("write cassette");

        unsafe {
            std::env::set_var("WRELA_HTTP_MODE", "replay");
            std::env::set_var("WRELA_CASSETTE_DIR", dir.path());
        }
        let service = str_value(service_name);
        let endpoint = str_value(endpoint_name);
        let method = str_value("POST");
        let url = str_value(url_value);
        let body = str_value(body_value);
        let headers = wr_map_new();
        let timeout_ms = Value::from_int(500);

        let result = wr_http_call(service, endpoint, method, url, headers, body, timeout_ms);
        assert!(!wr_result_is_ok(result).as_bool());
        let err = wr_result_err_unwrap(result);
        let err_text = value_to_string(err);
        assert!(err_text.contains("unsupported cassette version"));

        dec(service);
        dec(endpoint);
        dec(method);
        dec(url);
        dec(body);
        dec(headers);
        dec(result);
        dec(err);
    }

    #[test]
    fn runtime_configure_smoke() {
        let names = [b"actor_batch_limit".as_ptr()];
        let lens = [17usize];
        let cfg = wr_class_new(1001, names.as_ptr(), lens.as_ptr(), 1);
        wr_class_set(cfg, b"actor_batch_limit".as_ptr(), 17, Value::from_int(4));

        let result = wr_runtime_configure(cfg);

        dec(cfg);
        dec(result);
    }

    #[test]
    #[should_panic(expected = "actor_mailbox_cap")]
    fn runtime_configure_rejects_normalized_negative_capacity() {
        let names = [b"actor_mailbox_cap".as_ptr()];
        let lens = [17usize];
        let cfg = wr_class_new(1002, names.as_ptr(), lens.as_ptr(), 1);
        wr_class_set(cfg, b"actor_mailbox_cap".as_ptr(), 17, Value::from_int(-1));
        let _ = crate::config::runtime_configure(cfg);
    }

    #[test]
    fn actor_spawn_rejects_legacy_objective_fallback() {
        let actor = crate::actor::actor_spawn(1, Value::nil(), 1, 7, 256, 10, 64);
        assert!(actor.is_nil());
    }

    #[test]
    fn actor_spawn_legacy_default_sentinel_uses_runtime_config() {
        let actor = crate::actor::actor_spawn(1, Value::nil(), 1, 3, -1, 10, 64);
        assert!(!actor.is_nil());
        dec(actor);
    }

    #[test]
    fn class_slot_layout_and_dynamic_fallback_paths() {
        let names = [b"value".as_ptr(), b"count".as_ptr()];
        let lens = [5usize, 5usize];
        let obj = wr_class_new(1100, names.as_ptr(), lens.as_ptr(), 2);

        wr_class_set_slot(obj, b"ignored".as_ptr(), 7, 0, Value::from_int(41));
        wr_class_set_slot(obj, b"ignored".as_ptr(), 7, 1, Value::from_int(7));

        let value = wr_class_get_slot(obj, std::ptr::null(), 0, 0);
        let count = wr_class_get_slot(obj, std::ptr::null(), 0, 1);
        assert_eq!(value.as_int(), 41);
        assert_eq!(count.as_int(), 7);
        dec(value);
        dec(count);

        wr_class_set_slot(
            obj,
            b"ephemeral".as_ptr(),
            9,
            usize::MAX,
            Value::from_int(99),
        );
        let fallback = wr_class_get(obj, b"ephemeral".as_ptr(), 9);
        assert_eq!(fallback.as_int(), 99);
        dec(fallback);

        let by_name_slot = wr_class_get_slot(obj, b"value".as_ptr(), 5, usize::MAX);
        assert_eq!(by_name_slot.as_int(), 41);
        dec(by_name_slot);

        dec(obj);
    }

    #[test]
    #[ignore]
    fn class_slot_perf_microbench_artifact() {
        let names = [b"really_really_hot_field_name_for_lookup".as_ptr()];
        let lens = [37usize];
        let obj = wr_class_new(1200, names.as_ptr(), lens.as_ptr(), 1);
        wr_class_set_slot(obj, b"ignored".as_ptr(), 7, 0, Value::from_int(1));

        let iters = 1_000_000usize;
        for _ in 0..10_000 {
            let v = wr_class_get_slot(obj, std::ptr::null(), 0, 0);
            black_box(v.0);
            dec(v);
        }
        for _ in 0..10_000 {
            let v = wr_class_get(obj, b"really_really_hot_field_name_for_lookup".as_ptr(), 37);
            black_box(v.0);
            dec(v);
        }

        let slot_start = Instant::now();
        for _ in 0..iters {
            let v = wr_class_get_slot(obj, std::ptr::null(), 0, 0);
            black_box(v.0);
            dec(v);
        }
        let slot_elapsed = slot_start.elapsed();

        let fallback_start = Instant::now();
        for _ in 0..iters {
            let v = wr_class_get(obj, b"really_really_hot_field_name_for_lookup".as_ptr(), 37);
            black_box(v.0);
            dec(v);
        }
        let fallback_elapsed = fallback_start.elapsed();

        let slot_ns_per_op = slot_elapsed.as_nanos() as f64 / iters as f64;
        let fallback_ns_per_op = fallback_elapsed.as_nanos() as f64 / iters as f64;
        let improvement_pct = if fallback_ns_per_op > 0.0 {
            (fallback_ns_per_op - slot_ns_per_op) / fallback_ns_per_op * 100.0
        } else {
            0.0
        };

        let artifact_dir = std::path::Path::new(".artifacts/wre-407");
        std::fs::create_dir_all(&artifact_dir).expect("create artifact dir");
        let artifact_path = artifact_dir.join("class_slot_vs_fallback.txt");
        let body = format!(
            "iters={iters}\nslot_ns_per_op={slot_ns_per_op:.2}\nfallback_ns_per_op={fallback_ns_per_op:.2}\nimprovement_pct={improvement_pct:.2}\n"
        );
        std::fs::write(&artifact_path, body).expect("write perf artifact");

        dec(obj);
    }

    #[test]
    #[ignore]
    fn map_ic_hit_miss_perf_artifact() {
        let map = wr_map_new();
        let key_a = str_value("alpha");
        let key_b = str_value("beta");
        let val_a = Value::from_int(1);
        let val_b = Value::from_int(2);
        let _ = wr_map_set(map, key_a, val_a);
        let _ = wr_map_set(map, key_b, val_b);

        let iters = 1_000_000usize;
        for _ in 0..10_000 {
            let v = wr_map_get(map, key_a);
            black_box(v.0);
            dec(v);
        }
        crate::map::map_ic_reset_stats();
        let hit_start = Instant::now();
        for _ in 0..iters {
            let v = wr_map_get(map, key_a);
            black_box(v.0);
            dec(v);
        }
        let hit_elapsed = hit_start.elapsed();
        let (hit_hits, hit_misses) = crate::map::map_ic_stats();

        crate::map::map_ic_reset_stats();
        let miss_start = Instant::now();
        for i in 0..iters {
            let key = if i & 1 == 0 { key_a } else { key_b };
            let v = wr_map_get(map, key);
            black_box(v.0);
            dec(v);
        }
        let miss_elapsed = miss_start.elapsed();
        let (miss_hits, miss_misses) = crate::map::map_ic_stats();

        let hit_ns_per_op = hit_elapsed.as_nanos() as f64 / iters as f64;
        let miss_ns_per_op = miss_elapsed.as_nanos() as f64 / iters as f64;
        let hit_rate = if hit_hits + hit_misses > 0 {
            hit_hits as f64 / (hit_hits + hit_misses) as f64
        } else {
            0.0
        };
        let miss_rate = if miss_hits + miss_misses > 0 {
            miss_misses as f64 / (miss_hits + miss_misses) as f64
        } else {
            0.0
        };

        let artifact_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.artifacts/wre-415");
        std::fs::create_dir_all(&artifact_dir).expect("create artifact dir");
        let artifact_path = artifact_dir.join("map_ic_hit_miss.txt");
        let body = format!(
            "iters={iters}\nhit_ns_per_op={hit_ns_per_op:.2}\nmiss_ns_per_op={miss_ns_per_op:.2}\nhit_phase_hits={hit_hits}\nhit_phase_misses={hit_misses}\nhit_phase_hit_rate={hit_rate:.4}\nmiss_phase_hits={miss_hits}\nmiss_phase_misses={miss_misses}\nmiss_phase_miss_rate={miss_rate:.4}\n"
        );
        std::fs::write(&artifact_path, body).expect("write perf artifact");

        dec(map);
        dec(key_a);
        dec(key_b);
    }

    #[test]
    #[ignore]
    fn abi_lane_call_heavy_perf_artifact() {
        let _guard = abi_test_lock().lock().expect("abi test lock");
        let _metrics_guard = metrics::test_lock().lock().expect("metrics test lock");
        let iters = 2_000_000usize;
        let input = 987_654_321i64;

        unsafe {
            std::env::remove_var("WRELA_ABI_TYPED_FAST_PATH");
        }
        abi_refresh_typed_lane_cache();
        metrics::reset();
        let boxed_start = Instant::now();
        for _ in 0..iters {
            black_box(abi_roundtrip_i64(input));
        }
        let boxed_elapsed = boxed_start.elapsed();
        let boxed_ops = metrics::metrics_get_raw(metrics::METRIC_ABI_BOXED_LANE);

        let (typed_ns_per_op, typed_ops) = {
            #[cfg(feature = "abi_typed_fast_path")]
            {
                unsafe {
                    std::env::set_var("WRELA_ABI_TYPED_FAST_PATH", "1");
                }
                abi_refresh_typed_lane_cache();
                metrics::reset();
                let typed_start = Instant::now();
                for _ in 0..iters {
                    black_box(abi_roundtrip_i64(input));
                }
                let typed_elapsed = typed_start.elapsed();
                (
                    typed_elapsed.as_nanos() as f64 / iters as f64,
                    metrics::metrics_get_raw(metrics::METRIC_ABI_TYPED_LANE),
                )
            }
            #[cfg(not(feature = "abi_typed_fast_path"))]
            {
                (0.0f64, 0u64)
            }
        };

        let boxed_ns_per_op = boxed_elapsed.as_nanos() as f64 / iters as f64;
        let improvement_pct = if typed_ns_per_op > 0.0 {
            (boxed_ns_per_op - typed_ns_per_op) / boxed_ns_per_op * 100.0
        } else {
            0.0
        };

        let artifact_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(".artifacts/wre-411");
        std::fs::create_dir_all(&artifact_dir).expect("create artifact dir");
        let artifact_path = artifact_dir.join("abi_lane_call_heavy.txt");
        let body = format!(
            "iters={iters}\nboxed_ns_per_op={boxed_ns_per_op:.2}\ntyped_ns_per_op={typed_ns_per_op:.2}\nimprovement_pct={improvement_pct:.2}\nboxed_ops={boxed_ops}\ntyped_ops={typed_ops}\nfeature_abi_typed_fast_path={}\n",
            cfg!(feature = "abi_typed_fast_path")
        );
        std::fs::write(&artifact_path, body).expect("write perf artifact");
    }

    #[test]
    fn runtime_caps_export_is_non_zero() {
        let caps = wr_runtime_caps();
        assert_ne!(caps, 0);
        assert_eq!(
            caps & crate::unsafe_primitives::RUNTIME_CAP_ABI_NEGOTIATION_MARKER,
            crate::unsafe_primitives::RUNTIME_CAP_ABI_NEGOTIATION_MARKER
        );
    }
}
