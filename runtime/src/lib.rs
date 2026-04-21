#[cfg(not(target_pointer_width = "64"))]
compile_error!("wrela_runtime requires 64-bit targets");

// Keep jemalloc on Linux production targets; macOS dev/test builds use the
// system allocator to avoid native jemalloc build-script instability.
#[cfg(all(not(target_env = "msvc"), target_os = "linux"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

mod data;
pub mod domain_abi;
pub mod engine_executor;
mod host;
mod kernel;
pub mod reactor;
pub mod state_advance;
mod unsafe_primitives;
mod virtual_gpu;
pub mod wasm_runtime;

pub(crate) use data::{
    arena, bytes, class, iter, list, map, math, object, portable, result, string, value,
};
pub(crate) use kernel::{actor, config, diagnostics, metrics};

use data::object::drop_object;
use data::value::int_value;
pub use data::value::{TypeId, Value};

/// Returns the shared Tokio runtime for blocking on async work from sync context.
pub fn tokio_runtime() -> &'static tokio::runtime::Runtime {
    kernel::runtime::tokio_runtime()
}

use std::collections::HashMap;
#[cfg(test)]
use std::sync::atomic::AtomicU8;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

const WR_REACTOR_EVENT_READABLE: i32 = 1;
const WR_REACTOR_EVENT_WRITABLE: i32 = 2;
const WR_REACTOR_EVENT_TIMER: i32 = 3;
#[cfg(test)]
const ABI_TYPED_LANE_UNKNOWN: u8 = 0;
#[cfg(all(test, feature = "abi_typed_fast_path"))]
const ABI_TYPED_LANE_ENABLED: u8 = 1;
#[cfg(all(test, feature = "abi_typed_fast_path"))]
const ABI_TYPED_LANE_DISABLED: u8 = 2;

#[cfg(test)]
static ABI_TYPED_LANE_CACHE: AtomicU8 = AtomicU8::new(ABI_TYPED_LANE_UNKNOWN);

fn runtime_startup_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("WRELA_RUNTIME_STARTUP_TRACE")
            .ok()
            .map(|raw| {
                matches!(
                    raw.trim(),
                    "1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON"
                )
            })
            .unwrap_or(false)
    })
}

fn runtime_startup_trace(message: impl AsRef<str>) {
    if runtime_startup_trace_enabled() {
        eprintln!("[runtime-startup] {}", message.as_ref());
    }
}

fn next_positive_u64_handle(counter: &AtomicU64) -> Option<u64> {
    let max = i64::MAX as u64;
    loop {
        let current = counter.load(Ordering::Relaxed);
        if current == 0 || current > max {
            return None;
        }
        let next = if current == max { 0 } else { current + 1 };
        if counter
            .compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            return Some(current);
        }
    }
}

#[cold]
fn rc_invariant_violation(message: &str) -> ! {
    eprintln!("fatal: {message}");
    #[cfg(test)]
    panic!("{message}");
    #[cfg(not(test))]
    std::process::abort();
}

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

    fn insert(&self, reactor: reactor::Reactor) -> Option<i64> {
        let handle = next_positive_u64_handle(&self.next_handle)?;
        self.handles
            .lock()
            .expect("reactor registry lock")
            .insert(handle, Arc::new(reactor));
        Some(handle as i64)
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

    fn insert(&self, signal: reactor::task::TaskSignal) -> Option<i64> {
        let handle = next_positive_u64_handle(&self.next_handle)?;
        self.handles
            .lock()
            .expect("task signal registry lock")
            .insert(handle, Arc::new(signal));
        Some(handle as i64)
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

    fn insert(&self, value: i64) -> Option<i64> {
        let handle = next_positive_u64_handle(&self.next_handle)?;
        self.handles
            .lock()
            .expect("atomic registry lock")
            .insert(handle, Arc::new(AtomicI64::new(value)));
        Some(handle as i64)
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
/// # Safety
/// `value` must be a valid runtime `Value`. Pointer values must reference a live runtime
/// object with a valid header and reference count.
pub unsafe extern "C" fn wr_rc_inc(value: Value) {
    if let Err(message) = rc_inc_checked(value) {
        rc_invariant_violation(message);
    }
}

fn rc_inc_checked(value: Value) -> Result<(), &'static str> {
    if !value.is_ptr() {
        return Ok(());
    }
    if arena::is_arena_ptr(value.as_ptr()) {
        return Ok(());
    }
    metrics::inc_rc_inc();
    let header = unsafe { &*value.as_ptr() };
    loop {
        let current = header.rc.load(std::sync::atomic::Ordering::Relaxed);
        if current == 0 {
            return Err("wr_rc_inc called on object with rc=0 (use-after-free)");
        }
        if current == u32::MAX {
            return Err("wr_rc_inc overflow");
        }
        if header
            .rc
            .compare_exchange_weak(
                current,
                current + 1,
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
            )
            .is_ok()
        {
            return Ok(());
        }
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// `value` must be a valid runtime `Value`. Pointer values must reference a live runtime
/// object with a valid header and reference count.
pub unsafe extern "C" fn wr_rc_dec(value: Value) {
    match rc_dec_checked(value) {
        Ok(should_drop) => {
            if should_drop {
                std::sync::atomic::fence(std::sync::atomic::Ordering::Acquire);
                unsafe { drop_object(value.as_ptr()) };
            }
        }
        Err(message) => rc_invariant_violation(message),
    }
}

fn rc_dec_checked(value: Value) -> Result<bool, &'static str> {
    if !value.is_ptr() {
        return Ok(false);
    }
    if arena::is_arena_ptr(value.as_ptr()) {
        return Ok(false);
    }
    metrics::inc_rc_dec();
    let header = unsafe { &*value.as_ptr() };
    loop {
        let current = header.rc.load(std::sync::atomic::Ordering::Relaxed);
        if current == 0 {
            eprintln!(
                "fatal: wr_rc_dec invariant ptr={:p} type_id={}",
                value.as_ptr(),
                header.type_id
            );
            return Err("wr_rc_dec called on object with rc=0 (double-free)");
        }
        if header
            .rc
            .compare_exchange_weak(
                current,
                current - 1,
                std::sync::atomic::Ordering::Release,
                std::sync::atomic::Ordering::Relaxed,
            )
            .is_ok()
        {
            return Ok(current == 1);
        }
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
    runtime_startup_trace("wr_runtime_init: begin");
    diagnostics::runtime_init();
    #[cfg(feature = "metrics")]
    metrics::install_dump_hook();
    runtime_startup_trace("wr_runtime_init: ready");
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_runtime_abi() -> u32 {
    diagnostics::runtime_init();
    diagnostics::RUNTIME_ABI_VERSION
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_runtime_caps() -> u64 {
    unsafe_primitives::runtime_caps_mask()
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_reactor_new() -> Value {
    match reactor::Reactor::new() {
        Ok(reactor) => reactor_registry()
            .insert(reactor)
            .map(Value::from_int)
            .unwrap_or(Value::nil()),
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
/// # Safety
/// `out_token` and `out_kind` must be valid writable pointers for this call.
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
                    reactor::ReactorEventKind::Writable => WR_REACTOR_EVENT_WRITABLE,
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
    task_signal_registry()
        .insert(reactor::task::TaskSignal::new())
        .map(Value::from_int)
        .unwrap_or(Value::nil())
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
/// # Safety
/// `out_epoch` must be a valid writable pointer for this call.
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
    atomic_i64_registry()
        .insert(int_value(initial).unwrap_or(0))
        .map(Value::from_int)
        .unwrap_or(Value::nil())
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
pub extern "C" fn wr_approx_eq(a: Value, b: Value, tolerance: Value) -> Value {
    Value::from_bool(value::value_approx_eq(a, b, tolerance))
}

fn numeric_value_f64(val: Value) -> Option<f64> {
    if let Some(int) = int_value(val) {
        return Some(int as f64);
    }
    if val.is_float() {
        return Some(val.as_float());
    }
    None
}

fn cast_to_f32_value(val: Value) -> Value {
    numeric_value_f64(val)
        .map(|value| Value::from_float((value as f32) as f64))
        .unwrap_or(Value::nil())
}

fn cast_to_i32_value(val: Value) -> Value {
    numeric_value_f64(val)
        .map(|value| Value::from_int(value as i32 as i64))
        .unwrap_or(Value::nil())
}

fn cast_to_i64_value(val: Value) -> Value {
    numeric_value_f64(val)
        .map(|value| Value::from_int(value as i64))
        .unwrap_or(Value::nil())
}

fn cast_to_u32_value(val: Value) -> Value {
    numeric_value_f64(val)
        .map(|value| Value::from_int(value as u32 as i64))
        .unwrap_or(Value::nil())
}

fn cast_to_u64_value(val: Value) -> Value {
    numeric_value_f64(val)
        .map(|value| Value::from_int((value.max(0.0) as u64) as i64))
        .unwrap_or(Value::nil())
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

#[allow(clippy::not_unsafe_ptr_arg_deref)]
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
pub extern "C" fn wr_list_get_val(list_val: Value, idx_val: Value) -> Value {
    let idx = int_value(idx_val).unwrap_or(0).max(0) as usize;
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
pub extern "C" fn wr_gpu_buffer_new(len: Value, default_value: Value) -> Value {
    virtual_gpu::gpu_buffer_new(len, default_value)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_gpu_buffer_len(handle: Value) -> Value {
    virtual_gpu::gpu_buffer_len(handle)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_gpu_buffer_get(handle: Value, index: Value) -> Value {
    virtual_gpu::gpu_buffer_get(handle, index)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_gpu_buffer_set(handle: Value, index: Value, value: Value) -> Value {
    virtual_gpu::gpu_buffer_set(handle, index, value)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_gpu_global_invocation_id() -> Value {
    virtual_gpu::global_invocation_id()
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_gpu_local_invocation_id() -> Value {
    virtual_gpu::local_invocation_id()
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_gpu_workgroup_id() -> Value {
    virtual_gpu::workgroup_id()
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_gpu_num_workgroups() -> Value {
    virtual_gpu::num_workgroups()
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_gpu_workgroup_size() -> Value {
    virtual_gpu::workgroup_size()
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_gpu_dispatch_begin(
    num_workgroups_x: Value,
    num_workgroups_y: Value,
    num_workgroups_z: Value,
    workgroup_size_x: Value,
    workgroup_size_y: Value,
    workgroup_size_z: Value,
    schedule: Value,
) -> Value {
    virtual_gpu::dispatch_begin(
        num_workgroups_x,
        num_workgroups_y,
        num_workgroups_z,
        workgroup_size_x,
        workgroup_size_y,
        workgroup_size_z,
        schedule,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_gpu_dispatch_select_invocation(index: Value) -> Value {
    virtual_gpu::dispatch_select_invocation(index)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_gpu_dispatch_end() -> Value {
    virtual_gpu::dispatch_end()
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_gpu_atomic_i32_new(initial: Value) -> Value {
    virtual_gpu::gpu_atomic_i32_new(initial)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_gpu_atomic_i32_drop(handle: Value) -> Value {
    virtual_gpu::gpu_atomic_i32_drop(handle)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_gpu_atomic_i32_load(handle: Value) -> Value {
    virtual_gpu::gpu_atomic_i32_load(handle)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_gpu_atomic_i32_store(handle: Value, value: Value) -> Value {
    let _ = virtual_gpu::gpu_atomic_i32_store(handle, value);
    Value::nil()
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_gpu_atomic_i32_fetch_add(handle: Value, delta: Value) -> Value {
    virtual_gpu::gpu_atomic_i32_fetch_add(handle, delta)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_gpu_atomic_u32_new(initial: Value) -> Value {
    virtual_gpu::gpu_atomic_u32_new(initial)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_gpu_atomic_u32_drop(handle: Value) -> Value {
    virtual_gpu::gpu_atomic_u32_drop(handle)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_gpu_atomic_u32_load(handle: Value) -> Value {
    virtual_gpu::gpu_atomic_u32_load(handle)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_gpu_atomic_u32_store(handle: Value, value: Value) -> Value {
    let _ = virtual_gpu::gpu_atomic_u32_store(handle, value);
    Value::nil()
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_gpu_atomic_u32_fetch_add(handle: Value, delta: Value) -> Value {
    virtual_gpu::gpu_atomic_u32_fetch_add(handle, delta)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_gpu_schedule_deterministic() -> Value {
    virtual_gpu::gpu_schedule_deterministic()
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_gpu_schedule_reverse() -> Value {
    virtual_gpu::gpu_schedule_reverse()
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_gpu_schedule_shuffle(seed: Value) -> Value {
    virtual_gpu::gpu_schedule_shuffle(seed)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_gpu_schedule_workgroup_reverse() -> Value {
    virtual_gpu::gpu_schedule_workgroup_reverse()
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_gpu_schedule_workgroup_shuffle(seed: Value) -> Value {
    virtual_gpu::gpu_schedule_workgroup_shuffle(seed)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_gpu_schedule_round_robin_workgroups() -> Value {
    virtual_gpu::gpu_schedule_round_robin_workgroups()
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
pub extern "C" fn wr_cast_f32(val: Value) -> Value {
    cast_to_f32_value(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_cast_i32(val: Value) -> Value {
    cast_to_i32_value(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_cast_i64(val: Value) -> Value {
    cast_to_i64_value(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_cast_u32(val: Value) -> Value {
    cast_to_u32_value(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_cast_u64(val: Value) -> Value {
    cast_to_u64_value(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec2_new(x: Value, y: Value) -> Value {
    math::vec2_new(x, y)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec3_new(x: Value, y: Value, z: Value) -> Value {
    math::vec3_new(x, y, z)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec4_new(x: Value, y: Value, z: Value, w: Value) -> Value {
    math::vec4_new(x, y, z, w)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_quat_new(x: Value, y: Value, z: Value, w: Value) -> Value {
    math::quat_new(x, y, z, w)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec_component(val: Value, index: Value) -> Value {
    let Some(index) = int_value(index).filter(|v| *v >= 0).map(|v| v as usize) else {
        return Value::nil();
    };
    math::vec_component(val, index)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_mat3_component(val: Value, index: Value) -> Value {
    let Some(index) = int_value(index).filter(|v| *v >= 0).map(|v| v as usize) else {
        return Value::nil();
    };
    math::mat3_component(val, index)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_mat4_component(val: Value, index: Value) -> Value {
    let Some(index) = int_value(index).filter(|v| *v >= 0).map(|v| v as usize) else {
        return Value::nil();
    };
    math::mat4_component(val, index)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec2_x(val: Value) -> Value {
    math::vec_x(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec2_y(val: Value) -> Value {
    math::vec_y(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec3_x(val: Value) -> Value {
    math::vec_x(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec3_y(val: Value) -> Value {
    math::vec_y(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec3_z(val: Value) -> Value {
    math::vec_z(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec4_x(val: Value) -> Value {
    math::vec_x(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec4_y(val: Value) -> Value {
    math::vec_y(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec4_z(val: Value) -> Value {
    math::vec_z(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec4_w(val: Value) -> Value {
    math::vec_w(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_quat_x(val: Value) -> Value {
    math::quat_x(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_quat_y(val: Value) -> Value {
    math::quat_y(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_quat_z(val: Value) -> Value {
    math::quat_z(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_quat_w(val: Value) -> Value {
    math::quat_w(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec_add(a: Value, b: Value) -> Value {
    math::vec_add(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec_sub(a: Value, b: Value) -> Value {
    math::vec_sub(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec_mul(a: Value, b: Value) -> Value {
    math::vec_mul(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec_div(a: Value, b: Value) -> Value {
    math::vec_div(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec_dot(a: Value, b: Value) -> Value {
    math::vec_dot(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec_length(val: Value) -> Value {
    math::vec_length(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec_normalize(val: Value) -> Value {
    math::vec_normalize(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec_cross(a: Value, b: Value) -> Value {
    math::vec_cross(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec_min(a: Value, b: Value) -> Value {
    math::vec_min(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec_max(a: Value, b: Value) -> Value {
    math::vec_max(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec_clamp(value: Value, min: Value, max: Value) -> Value {
    math::vec_clamp(value, min, max)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec_mix(a: Value, b: Value, t: Value) -> Value {
    math::vec_mix(a, b, t)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec_abs(val: Value) -> Value {
    math::vec_abs(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec_sign(val: Value) -> Value {
    math::vec_sign(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec_floor(val: Value) -> Value {
    math::vec_floor(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec_ceil(val: Value) -> Value {
    math::vec_ceil(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec_fract(val: Value) -> Value {
    math::vec_fract(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec_sin(val: Value) -> Value {
    math::vec_sin(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec_cos(val: Value) -> Value {
    math::vec_cos(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec_sqrt(val: Value) -> Value {
    math::vec_sqrt(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec_pow(a: Value, b: Value) -> Value {
    math::vec_pow(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec_distance(a: Value, b: Value) -> Value {
    math::vec_distance(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_vec_reflect(incident: Value, normal: Value) -> Value {
    math::vec_reflect(incident, normal)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_quat_add(a: Value, b: Value) -> Value {
    math::vec_add(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_quat_sub(a: Value, b: Value) -> Value {
    math::vec_sub(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_quat_mul(a: Value, b: Value) -> Value {
    math::vec_mul(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_quat_div(a: Value, b: Value) -> Value {
    math::vec_div(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_quat_dot(a: Value, b: Value) -> Value {
    math::vec_dot(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_quat_length(val: Value) -> Value {
    math::vec_length(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_quat_normalize(val: Value) -> Value {
    math::vec_normalize(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_quat_min(a: Value, b: Value) -> Value {
    math::vec_min(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_quat_max(a: Value, b: Value) -> Value {
    math::vec_max(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_quat_clamp(value: Value, min: Value, max: Value) -> Value {
    math::vec_clamp(value, min, max)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_quat_mix(a: Value, b: Value, t: Value) -> Value {
    math::vec_mix(a, b, t)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_quat_abs(val: Value) -> Value {
    math::vec_abs(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_quat_sign(val: Value) -> Value {
    math::vec_sign(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_quat_floor(val: Value) -> Value {
    math::vec_floor(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_quat_ceil(val: Value) -> Value {
    math::vec_ceil(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_quat_fract(val: Value) -> Value {
    math::vec_fract(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_quat_sin(val: Value) -> Value {
    math::vec_sin(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_quat_cos(val: Value) -> Value {
    math::vec_cos(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_quat_sqrt(val: Value) -> Value {
    math::vec_sqrt(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_quat_pow(a: Value, b: Value) -> Value {
    math::vec_pow(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_mat3_identity() -> Value {
    math::mat3_identity()
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_mat3_from_columns(c0: Value, c1: Value, c2: Value) -> Value {
    math::mat3_from_columns(c0, c1, c2)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_mat3_mul_vec3(mat: Value, vec: Value) -> Value {
    math::mat3_mul_vec3(mat, vec)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_mat3_mul_mat3(a: Value, b: Value) -> Value {
    math::mat3_mul_mat3(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_mat3_add(a: Value, b: Value) -> Value {
    math::mat3_add(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_mat3_sub(a: Value, b: Value) -> Value {
    math::mat3_sub(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_mat3_mul_scalar(mat: Value, scalar: Value) -> Value {
    math::mat3_mul_scalar(mat, scalar)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_mat3_div_scalar(mat: Value, scalar: Value) -> Value {
    math::mat3_div_scalar(mat, scalar)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_mat4_identity() -> Value {
    math::mat4_identity()
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_mat4_from_columns(c0: Value, c1: Value, c2: Value, c3: Value) -> Value {
    math::mat4_from_columns(c0, c1, c2, c3)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_mat4_mul_vec4(mat: Value, vec: Value) -> Value {
    math::mat4_mul_vec4(mat, vec)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_mat4_mul_mat4(a: Value, b: Value) -> Value {
    math::mat4_mul_mat4(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_mat4_add(a: Value, b: Value) -> Value {
    math::mat4_add(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_mat4_sub(a: Value, b: Value) -> Value {
    math::mat4_sub(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_mat4_mul_scalar(mat: Value, scalar: Value) -> Value {
    math::mat4_mul_scalar(mat, scalar)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_mat4_div_scalar(mat: Value, scalar: Value) -> Value {
    math::mat4_div_scalar(mat, scalar)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_bounds2_center(bounds: Value) -> Value {
    portable::bounds2_center(bounds)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_bounds2_size(bounds: Value) -> Value {
    portable::bounds2_size(bounds)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_bounds3_center(bounds: Value) -> Value {
    portable::bounds3_center(bounds)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_bounds3_size(bounds: Value) -> Value {
    portable::bounds3_size(bounds)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_transform_point(transform: Value, point: Value) -> Value {
    portable::transform_point(transform, point)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_field_transform_point(transform: Value, point: Value) -> Value {
    portable::field_transform_point(transform, point)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_field_rotate_point(rotation: Value, point: Value) -> Value {
    portable::field_rotate_point(rotation, point)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_field_instance_point(instance: Value, point: Value) -> Value {
    portable::field_instance_point(instance, point)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_field_mirror_point(mirror: Value, point: Value) -> Value {
    portable::field_mirror_point(mirror, point)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_field_repeat_point(period: Value, point: Value) -> Value {
    portable::field_repeat_point(period, point)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_field_sweep_coords(path: Value, point: Value) -> Value {
    portable::field_sweep_coords(path, point)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_field_profile_vertices_bounds4(vertices: Value) -> Value {
    portable::field_profile_vertices_bounds4(vertices)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_translate(offset: Value, point: Value) -> Value {
    portable::translate(offset, point)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_rotate(rotation: Value, point: Value) -> Value {
    portable::rotate(rotation, point)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_uniform_scale(scale: Value, point: Value) -> Value {
    portable::uniform_scale(scale, point)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_affine_transform(transform: Value, point: Value) -> Value {
    portable::affine_transform(transform, point)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_warp(transform: Value, point: Value) -> Value {
    portable::warp(transform, point)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_repeat_linear(period: Value, point: Value) -> Value {
    portable::repeat_linear(period, point)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_repeat_linear_identity(period: Value, point: Value) -> Value {
    portable::repeat_linear_identity(period, point)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_repeat_grid(period: Value, point: Value) -> Value {
    portable::repeat_grid(period, point)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_repeat_grid_identity(period: Value, point: Value) -> Value {
    portable::repeat_grid_identity(period, point)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_radial_repeat(period: Value, point: Value) -> Value {
    portable::radial_repeat(period, point)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_radial_repeat_identity(period: Value, point: Value) -> Value {
    portable::radial_repeat_identity(period, point)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_mirror_array(mirror: Value, point: Value) -> Value {
    portable::mirror_array(mirror, point)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_mirror_array_identity(mirror: Value, point: Value) -> Value {
    portable::mirror_array_identity(mirror, point)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_instance_array(instance: Value, point: Value) -> Value {
    portable::instance_array(instance, point)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_instance_array_identity(instance: Value, point: Value) -> Value {
    portable::instance_array_identity(instance, point)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_transform_vector(transform: Value, vector: Value) -> Value {
    portable::transform_vector(transform, vector)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_transform_normal(transform: Value, normal: Value) -> Value {
    portable::transform_normal(transform, normal)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_transform3_identity(class_id: Value) -> Value {
    portable::transform3_identity(int_value(class_id).unwrap_or_default().max(0) as u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_compose_transform3(class_id: Value, left: Value, right: Value) -> Value {
    portable::compose_transform3(
        int_value(class_id).unwrap_or_default().max(0) as u32,
        left,
        right,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_inverse_transform3(class_id: Value, transform: Value) -> Value {
    portable::inverse_transform3(
        int_value(class_id).unwrap_or_default().max(0) as u32,
        transform,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_field_union(left: Value, right: Value) -> Value {
    portable::field_union(left, right)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_field_intersection(left: Value, right: Value) -> Value {
    portable::field_intersection(left, right)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_field_subtract(left: Value, right: Value) -> Value {
    portable::field_subtract(left, right)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_smooth_union(left: Value, right: Value, k: Value) -> Value {
    portable::smooth_union(left, right, k)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_smooth_intersection(left: Value, right: Value, k: Value) -> Value {
    portable::smooth_intersection(left, right, k)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_smooth_subtract(left: Value, right: Value, k: Value) -> Value {
    portable::smooth_subtract(left, right, k)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_bend(config: Value, point: Value) -> Value {
    portable::bend(config, point)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_twist(config: Value, point: Value) -> Value {
    portable::twist(config, point)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_taper(config: Value, point: Value) -> Value {
    portable::taper(config, point)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_displace(config: Value, point: Value) -> Value {
    portable::displace(config, point)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_repeat_point(point: Value, period: Value) -> Value {
    portable::repeat_point(point, period)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_sphere(point: Value, radius: Value) -> Value {
    portable::sphere(point, radius)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_box(point: Value, half: Value) -> Value {
    portable::box_sdf(point, half)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_capsule(point: Value, a: Value, b: Value, radius: Value) -> Value {
    portable::capsule(point, a, b, radius)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_cylinder(point: Value, radius: Value, half_height: Value) -> Value {
    portable::cylinder(point, radius, half_height)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_plane(point: Value, normal: Value, offset: Value) -> Value {
    portable::plane(point, normal, offset)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_torus(point: Value, major_radius: Value, minor_radius: Value) -> Value {
    portable::torus(point, major_radius, minor_radius)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_rounded_box(point: Value, half: Value, radius: Value) -> Value {
    portable::rounded_box(point, half, radius)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_circle2(point: Value, radius: Value) -> Value {
    portable::circle2(point, radius)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_rect2(point: Value, half: Value) -> Value {
    portable::rect2(point, half)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_rounded_rect2(point: Value, half: Value, radius: Value) -> Value {
    portable::rounded_rect2(point, half, radius)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_capsule2(point: Value, a: Value, b: Value, radius: Value) -> Value {
    portable::capsule2(point, a, b, radius)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_segment2(point: Value, a: Value, b: Value) -> Value {
    portable::segment2(point, a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_polygon2(point: Value, vertices: Value) -> Value {
    portable::polygon2(point, vertices)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_polyline2(point: Value, vertices: Value) -> Value {
    portable::polyline2(point, vertices)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_ellipsoid(point: Value, radii: Value) -> Value {
    portable::ellipsoid(point, radii)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_cone(point: Value, radius: Value, half_height: Value) -> Value {
    portable::cone(point, radius, half_height)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_capped_cone(
    point: Value,
    radius_bottom: Value,
    radius_top: Value,
    half_height: Value,
) -> Value {
    portable::capped_cone(point, radius_bottom, radius_top, half_height)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_box_frame(point: Value, half: Value, thickness: Value) -> Value {
    portable::box_frame(point, half, thickness)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_slab(point: Value, thickness: Value) -> Value {
    portable::slab(point, thickness)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_triangle_prism(point: Value, half: Value, half_height: Value) -> Value {
    portable::triangle_prism(point, half, half_height)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_hex_prism(point: Value, half: Value, half_height: Value) -> Value {
    portable::hex_prism(point, half, half_height)
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

fn deep_eq(a: Value, b: Value) -> bool {
    value::value_eq(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_value_deep_eq(left: Value, right: Value) -> Value {
    Value::from_bool(deep_eq(left, right))
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_assert_value_equality(left: Value, right: Value) -> Value {
    if deep_eq(left, right) {
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
    let output = result::result_is_ok(val);
    runtime_startup_trace(format!(
        "wr_result_is_ok: input_type_id={} output_type_id={} output_int={:?}",
        wr_type_id(val),
        wr_type_id(output),
        int_value(output)
    ));
    output
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_result_unwrap(val: Value) -> Value {
    let output = result::result_unwrap(val);
    runtime_startup_trace(format!(
        "wr_result_unwrap: input_type_id={} output_type_id={} output_int={:?}",
        wr_type_id(val),
        wr_type_id(output),
        int_value(output)
    ));
    output
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
    runtime_startup_trace(format!(
        "wr_actor_spawn: class_id={class_id} pool_size={pool_size} objective={objective} mailbox_cap={mailbox_cap} enqueue_timeout_ms={enqueue_timeout_ms} batch_limit={batch_limit}"
    ));
    let result = actor::actor_spawn(
        class_id,
        instance,
        pool_size,
        objective,
        mailbox_cap,
        enqueue_timeout_ms,
        batch_limit,
    );
    if let Some(handle) = int_value(result) {
        runtime_startup_trace(format!("wr_actor_spawn: handle={handle}"));
    } else {
        runtime_startup_trace("wr_actor_spawn: handle=nil");
    }
    result
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
    runtime_startup_trace(format!(
        "wr_pool_new: objective={objective} min_size={min_size} max_size={max_size} weight={weight} queue_cap={queue_cap}"
    ));
    let result = actor::pool_new(handles, objective, min_size, max_size, weight, queue_cap);
    if let Some(handle) = int_value(result) {
        runtime_startup_trace(format!("wr_pool_new: handle={handle}"));
    } else {
        runtime_startup_trace("wr_pool_new: handle=nil");
    }
    result
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
pub extern "C" fn wr_metrics_scene_trace() -> Value {
    metrics::inc_scene_trace();
    Value::nil()
}

pub fn metrics_add_scene_trace(count: u64) {
    metrics::add_scene_trace(count);
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_metrics_field_sample() -> Value {
    metrics::inc_field_sample();
    Value::nil()
}

pub fn metrics_add_field_samples(count: u64) {
    metrics::add_field_samples(count);
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_metrics_scene_trace_support_pruned_branch() -> Value {
    metrics::inc_scene_trace_support_pruned_branch();
    Value::nil()
}

pub fn metrics_add_scene_trace_support_pruned_branches(count: u64) {
    metrics::add_scene_trace_support_pruned_branches(count);
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_metrics_scene_trace_candidate_branch() -> Value {
    metrics::inc_scene_trace_candidate_branch();
    Value::nil()
}

pub fn metrics_add_scene_trace_candidate_branches(count: u64) {
    metrics::add_scene_trace_candidate_branches(count);
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_metrics_scene_trace_exact_path() -> Value {
    metrics::inc_scene_trace_exact_path();
    Value::nil()
}

pub fn metrics_add_scene_trace_exact_paths(count: u64) {
    metrics::add_scene_trace_exact_paths(count);
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_metrics_scene_trace_conservative_path() -> Value {
    metrics::inc_scene_trace_conservative_path();
    Value::nil()
}

pub fn metrics_add_scene_trace_conservative_paths(count: u64) {
    metrics::add_scene_trace_conservative_paths(count);
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_metrics_scene_trace_blend_cost() -> Value {
    metrics::inc_scene_trace_blend_cost();
    Value::nil()
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_metrics_scene_trace_deformation_cost() -> Value {
    metrics::inc_scene_trace_deformation_cost();
    Value::nil()
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_metrics_scene_trace_hit(steps_val: Value, field_samples_val: Value) -> Value {
    let steps = int_value(steps_val).unwrap_or(0).max(0) as u64;
    let field_samples = int_value(field_samples_val).unwrap_or(0).max(0) as u64;
    metrics::inc_scene_trace_hit(steps, field_samples);
    Value::nil()
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
pub extern "C" fn wr_metrics_scene_trace_id() -> Value {
    Value::from_int(metrics::METRIC_SCENE_TRACE as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_metrics_field_sample_id() -> Value {
    Value::from_int(metrics::METRIC_FIELD_SAMPLE as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_metrics_scene_trace_support_pruned_branch_id() -> Value {
    Value::from_int(metrics::METRIC_SCENE_TRACE_SUPPORT_PRUNED_BRANCH as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_metrics_scene_trace_candidate_branch_id() -> Value {
    Value::from_int(metrics::METRIC_SCENE_TRACE_CANDIDATE_BRANCH as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_metrics_scene_trace_exact_path_id() -> Value {
    Value::from_int(metrics::METRIC_SCENE_TRACE_EXACT_PATH as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_metrics_scene_trace_conservative_path_id() -> Value {
    Value::from_int(metrics::METRIC_SCENE_TRACE_CONSERVATIVE_PATH as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_metrics_scene_trace_hit_count_id() -> Value {
    Value::from_int(metrics::METRIC_SCENE_TRACE_HIT_COUNT as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_metrics_scene_trace_hit_steps_total_id() -> Value {
    Value::from_int(metrics::METRIC_SCENE_TRACE_HIT_STEPS_TOTAL as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_metrics_scene_trace_hit_field_samples_total_id() -> Value {
    Value::from_int(metrics::METRIC_SCENE_TRACE_HIT_FIELD_SAMPLES_TOTAL as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_metrics_scene_trace_steps_le_1_id() -> Value {
    Value::from_int(metrics::METRIC_SCENE_TRACE_STEPS_LE_1 as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_metrics_scene_trace_steps_le_4_id() -> Value {
    Value::from_int(metrics::METRIC_SCENE_TRACE_STEPS_LE_4 as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_metrics_scene_trace_steps_le_8_id() -> Value {
    Value::from_int(metrics::METRIC_SCENE_TRACE_STEPS_LE_8 as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_metrics_scene_trace_steps_le_16_id() -> Value {
    Value::from_int(metrics::METRIC_SCENE_TRACE_STEPS_LE_16 as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_metrics_scene_trace_steps_gt_16_id() -> Value {
    Value::from_int(metrics::METRIC_SCENE_TRACE_STEPS_GT_16 as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_metrics_scene_trace_blend_cost_id() -> Value {
    Value::from_int(metrics::METRIC_SCENE_TRACE_BLEND_COST as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_metrics_scene_trace_deformation_cost_id() -> Value {
    Value::from_int(metrics::METRIC_SCENE_TRACE_DEFORMATION_COST as i64)
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
pub extern "C" fn wr_list_set_val(list_val: Value, idx_val: Value, val: Value) -> Value {
    let idx = int_value(idx_val).unwrap_or(0).max(0) as usize;
    list::list_set(list_val, idx, val);
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
pub extern "C" fn wr_actor_send_0(handle: Value, method_id: u32) -> Value {
    actor::actor_send(handle, method_id, 0, std::ptr::null())
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_actor_send_1(handle: Value, method_id: u32, a1: Value) -> Value {
    let args = [a1];
    actor::actor_send(handle, method_id, 1, args.as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_actor_send_2(handle: Value, method_id: u32, a1: Value, a2: Value) -> Value {
    let args = [a1, a2];
    actor::actor_send(handle, method_id, 2, args.as_ptr())
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
pub extern "C" fn wr_actor_fire_0(handle: Value, method_id: u32) {
    actor::actor_fire(handle, method_id, 0, std::ptr::null())
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_actor_fire_1(handle: Value, method_id: u32, a1: Value) {
    let args = [a1];
    actor::actor_fire(handle, method_id, 1, args.as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_actor_fire_2(handle: Value, method_id: u32, a1: Value, a2: Value) {
    let args = [a1, a2];
    actor::actor_fire(handle, method_id, 2, args.as_ptr())
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
mod tests {
    use crate::*;
    use sha2::Digest;
    use std::hint::black_box;
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

    fn value_hash_u64(input: Value) -> u64 {
        use std::hash::Hasher;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        crate::value::value_hash(input, &mut hasher);
        hasher.finish()
    }

    fn dec(input: Value) {
        unsafe {
            wr_rc_dec(input);
        }
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
    fn class_overflow_set_self_assignment_keeps_value_alive() {
        let obj = wr_class_new(1300, std::ptr::null(), std::ptr::null(), 0);
        let key = b"field";
        let value = str_value("stable");

        wr_class_set(obj, key.as_ptr(), key.len(), value);
        // Drop our local ownership so the class entry is the sole owner (rc=1).
        dec(value);

        // Regression: second set with same value must not free before re-incrementing.
        wr_class_set(obj, key.as_ptr(), key.len(), value);
        let got = wr_class_get(obj, key.as_ptr(), key.len());
        assert_eq!(value_to_string(got), "stable");

        dec(got);
        dec(obj);
    }

    #[test]
    fn class_values_are_structurally_equal_and_hash_consistent() {
        let names = [b"id".as_ptr(), b"meta".as_ptr()];
        let lens = [2usize, 4usize];
        let left = wr_class_new(1400, names.as_ptr(), lens.as_ptr(), 2);
        let right = wr_class_new(1400, names.as_ptr(), lens.as_ptr(), 2);

        let left_meta = wr_map_new();
        let right_meta = wr_map_new();
        let left_key = str_value("k");
        let right_key = str_value("k");
        let _ = wr_map_set(left_meta, left_key, Value::from_int(2));
        let _ = wr_map_set(right_meta, right_key, Value::from_int(2));
        dec(left_key);
        dec(right_key);

        wr_class_set_slot(left, std::ptr::null(), 0, 0, Value::from_int(1));
        wr_class_set_slot(right, std::ptr::null(), 0, 0, Value::from_int(1));
        wr_class_set_slot(left, std::ptr::null(), 0, 1, left_meta);
        wr_class_set_slot(right, std::ptr::null(), 0, 1, right_meta);

        let eq = wr_value_eq(left, right);
        assert!(
            eq.is_bool() && eq.as_bool(),
            "class values should compare structurally"
        );

        let left_hash = value_hash_u64(left);
        let right_hash = value_hash_u64(right);
        assert_eq!(left_hash, right_hash, "equal values must hash equally");

        let rendered = wr_str_concat([left].as_ptr(), 1);
        assert_eq!(value_to_string(rendered), "Class#1400{id: 1, meta: {k: 2}}");

        dec(rendered);
        dec(eq);
        dec(left_meta);
        dec(right_meta);
        dec(left);
        dec(right);
    }

    #[test]
    fn class_rendering_is_deterministic_and_not_opaque() {
        let obj = wr_class_new(1401, std::ptr::null(), std::ptr::null(), 0);
        wr_class_set(obj, b"z".as_ptr(), 1, Value::from_int(9));
        wr_class_set(obj, b"a".as_ptr(), 1, Value::from_int(1));

        let rendered = wr_str_concat([obj].as_ptr(), 1);
        let text = value_to_string(rendered);
        assert_eq!(text, "Class#1401{a: 1, z: 9}");
        assert!(!text.contains("<obj>"));

        dec(rendered);
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
        std::fs::create_dir_all(artifact_dir).expect("create artifact dir");
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

    #[test]
    fn gpu_atomic_exports_round_trip() {
        let handle = wr_gpu_atomic_i32_new(Value::from_int(5));
        assert!(handle.is_int());
        assert!(int_value(wr_gpu_atomic_i32_load(handle)) == Some(5));
        assert!(int_value(wr_gpu_atomic_i32_fetch_add(handle, Value::from_int(7))) == Some(5));
        assert!(int_value(wr_gpu_atomic_i32_load(handle)) == Some(12));
        assert!(wr_gpu_atomic_i32_store(handle, Value::from_int(99)).is_nil());
        assert!(int_value(wr_gpu_atomic_i32_load(handle)) == Some(99));
        assert!(wr_gpu_atomic_i32_drop(handle) == Value::from_bool(true));

        let handle = wr_gpu_atomic_u32_new(Value::from_int(11));
        assert!(handle.is_int());
        assert!(int_value(wr_gpu_atomic_u32_load(handle)) == Some(11));
        assert!(int_value(wr_gpu_atomic_u32_fetch_add(handle, Value::from_int(2))) == Some(11));
        assert!(int_value(wr_gpu_atomic_u32_load(handle)) == Some(13));
        assert!(wr_gpu_atomic_u32_store(handle, Value::from_int(123)).is_nil());
        assert!(int_value(wr_gpu_atomic_u32_load(handle)) == Some(123));
        assert!(wr_gpu_atomic_u32_drop(handle) == Value::from_bool(true));
    }

    #[test]
    fn gpu_schedule_exports_drive_reverse_dispatch() {
        let schedule = wr_gpu_schedule_reverse();
        assert!(schedule.is_int());
        wr_gpu_dispatch_begin(
            Value::from_int(2),
            Value::from_int(1),
            Value::from_int(1),
            Value::from_int(2),
            Value::from_int(1),
            Value::from_int(1),
            schedule,
        );
        wr_gpu_dispatch_select_invocation(Value::from_int(0));
        let gid = wr_gpu_global_invocation_id();
        assert_eq!(int_value(crate::list::list_get(gid, 0)), Some(3));
        wr_gpu_dispatch_end();
    }

    #[test]
    fn gpu_schedule_exports_return_immediate_values() {
        let deterministic = wr_gpu_schedule_deterministic();
        let reverse = wr_gpu_schedule_reverse();
        let workgroup_reverse = wr_gpu_schedule_workgroup_reverse();
        let round_robin = wr_gpu_schedule_round_robin_workgroups();

        assert!(deterministic.is_int());
        assert!(reverse.is_int());
        assert!(workgroup_reverse.is_int());
        assert!(round_robin.is_int());
        assert!(deterministic != reverse);
        assert!(reverse != workgroup_reverse);
        assert!(workgroup_reverse != round_robin);
    }

    #[test]
    fn gpu_schedule_exports_drive_round_robin_dispatch() {
        let schedule = wr_gpu_schedule_round_robin_workgroups();
        wr_gpu_dispatch_begin(
            Value::from_int(2),
            Value::from_int(1),
            Value::from_int(1),
            Value::from_int(2),
            Value::from_int(1),
            Value::from_int(1),
            schedule,
        );

        wr_gpu_dispatch_select_invocation(Value::from_int(0));
        let gid0 = wr_gpu_global_invocation_id();
        assert_eq!(int_value(crate::list::list_get(gid0, 0)), Some(0));

        wr_gpu_dispatch_select_invocation(Value::from_int(1));
        let gid1 = wr_gpu_global_invocation_id();
        assert_eq!(int_value(crate::list::list_get(gid1, 0)), Some(2));

        wr_gpu_dispatch_select_invocation(Value::from_int(2));
        let gid2 = wr_gpu_global_invocation_id();
        assert_eq!(int_value(crate::list::list_get(gid2, 0)), Some(1));

        wr_gpu_dispatch_select_invocation(Value::from_int(3));
        let gid3 = wr_gpu_global_invocation_id();
        assert_eq!(int_value(crate::list::list_get(gid3, 0)), Some(3));

        wr_gpu_dispatch_end();
    }
}
