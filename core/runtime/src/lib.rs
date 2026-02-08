#![allow(clippy::missing_safety_doc)]

mod actor;
pub mod arena;
mod bytes;
mod class;
mod config;
mod diagnostics;
mod env;
mod float_box;
mod iter;
mod lease;
mod list;
mod logging;
mod map;
mod metrics;
mod number;
mod object;
mod range;
mod result;
mod scheduler;
mod string;
mod value;

use value::int_value;
pub use value::{TypeId, Value};

use object::drop_object;
use std::collections::HashSet;
use std::sync::OnceLock;
use std::time::Instant;

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
    number::num_add(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_num_sub(a: Value, b: Value) -> Value {
    number::num_sub(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_num_mul(a: Value, b: Value) -> Value {
    number::num_mul(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_num_div(a: Value, b: Value) -> Value {
    number::num_div(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_num_mod(a: Value, b: Value) -> Value {
    number::num_mod(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_num_neg(a: Value) -> Value {
    number::num_neg(a)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_num_lt(a: Value, b: Value) -> Value {
    number::num_lt(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_num_gt(a: Value, b: Value) -> Value {
    number::num_gt(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_num_le(a: Value, b: Value) -> Value {
    number::num_le(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_num_ge(a: Value, b: Value) -> Value {
    number::num_ge(a, b)
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
    Value::from_int(count.max(1))
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
    range::range_new(start, end)
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
    env::env_get(key)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_env_set(key: Value, val: Value) -> Value {
    env::env_set(key, val)
}

#[cfg(test)]
mod tests;
