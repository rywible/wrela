#![allow(clippy::missing_safety_doc)]

mod value;
mod object;
mod string;
mod list;
mod map;
mod float_box;
mod actor;
mod iter;
mod class;
mod metrics;
mod result;
mod number;
mod range;
mod config;
mod scheduler;
mod diagnostics;
pub mod storage;

pub use value::{TypeId, Value};
use value::int_value;

use object::drop_object;
use std::sync::OnceLock;
use std::time::Instant;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wr_rc_inc(value: Value) {
    if !value.is_ptr() {
        return;
    }
    metrics::inc_rc_inc();
    let header = unsafe { &*value.as_ptr() };
    header
        .rc
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wr_rc_dec(value: Value) {
    if !value.is_ptr() {
        return;
    }
    metrics::inc_rc_dec();
    let header = unsafe { &*value.as_ptr() };
    let next = header
        .rc
        .fetch_sub(1, std::sync::atomic::Ordering::Release)
        - 1;
    if next == 0 {
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
    if val.is_float() {
        val.as_float()
    } else {
        0.0
    }
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
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_runtime_abi() -> u32 {
    diagnostics::runtime_init();
    diagnostics::RUNTIME_ABI_VERSION
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_type_id(val: Value) -> u32 {
    value::type_id_raw(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_value_eq(a: Value, b: Value) -> Value {
    Value::from_bool(value::value_eq(a, b))
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
pub extern "C" fn wr_list_new(len: usize) -> Value {
    list::list_new(len)
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
pub extern "C" fn wr_map_get(map_val: Value, key: Value) -> Value {
    map::map_get(map_val, key)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_map_set(map_val: Value, key: Value, val: Value) {
    map::map_set(map_val, key, val)
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
pub extern "C" fn wr_assert(cond: Value, msg: Value) -> Value {
    let ok = if cond.is_bool() { cond.as_bool() } else { false };
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

fn builtin_error(message: &str) -> Value {
    string::str_from_utf8(message.as_ptr(), message.len())
}

fn string_bytes(val: Value) -> Option<Vec<u8>> {
    string::with_string_bytes(val, |bytes| bytes.to_vec())
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_parse_int(val: Value) -> Value {
    let Some(bytes) = string_bytes(val) else {
        return result::result_err(builtin_error("parse_int expects a String"));
    };
    let parsed = std::str::from_utf8(&bytes)
        .ok()
        .and_then(|s| s.trim().parse::<i64>().ok());
    match parsed {
        Some(num) => result::result_ok(Value::from_int(num)),
        None => result::result_err(builtin_error("parse_int: invalid integer")),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_parse_float(val: Value) -> Value {
    let Some(bytes) = string_bytes(val) else {
        return result::result_err(builtin_error("parse_float expects a String"));
    };
    let parsed = std::str::from_utf8(&bytes)
        .ok()
        .and_then(|s| s.trim().parse::<f64>().ok());
    match parsed {
        Some(num) => result::result_ok(Value::from_float(num)),
        None => result::result_err(builtin_error("parse_float: invalid float")),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_read_file(path: Value) -> Value {
    let Some(bytes) = string_bytes(path) else {
        return result::result_err(builtin_error("read_file expects a String"));
    };
    let path_str = String::from_utf8_lossy(&bytes);
    match std::fs::read(path_str.as_ref()) {
        Ok(contents) => match std::str::from_utf8(&contents) {
            Ok(text) => result::result_ok(string::str_from_utf8(text.as_ptr(), text.len())),
            Err(_) => result::result_err(builtin_error("read_file: invalid utf8")),
        },
        Err(err) => result::result_err(builtin_error(&format!("read_file: {err}"))),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_write_file(path: Value, contents: Value) -> Value {
    let Some(path_bytes) = string_bytes(path) else {
        return result::result_err(builtin_error("write_file expects a String path"));
    };
    let Some(contents_bytes) = string_bytes(contents) else {
        return result::result_err(builtin_error("write_file expects String contents"));
    };
    let path_str = String::from_utf8_lossy(&path_bytes);
    match std::fs::write(path_str.as_ref(), contents_bytes) {
        Ok(()) => result::result_ok(Value::nil()),
        Err(err) => result::result_err(builtin_error(&format!("write_file: {err}"))),
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
pub extern "C" fn wr_pool_auto_size(
    objective: Value,
    min: Value,
    max: Value,
    weight: Value,
) -> Value {
    let obj = int_value(objective).unwrap_or(0);
    let min = int_value(min).unwrap_or(0);
    let max = int_value(max).unwrap_or(0);
    let weight = int_value(weight).unwrap_or(0);
    Value::from_int(
        config::pool_auto_size(
            config::normalize_objective(obj),
            min,
            max,
            weight,
        ) as i64,
    )
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
pub extern "C" fn wr_actor_send(handle: Value, method_id: u32, argc: usize, argv_ptr: *const Value) -> Value {
    actor::actor_send(handle, method_id, argc, argv_ptr)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_actor_fire(handle: Value, method_id: u32, argc: usize, argv_ptr: *const Value) {
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
pub extern "C" fn wr_iter_init(iterable: Value) -> Value {
    iter::iter_init(iterable)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_iter_next(iter_val: Value, dst_value: *mut Value, dst_done: *mut Value) {
    iter::iter_next(iter_val, dst_value, dst_done)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_class_new(class_id: u32, names_ptr: *const *const u8, lens_ptr: *const usize, count: usize) -> Value {
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
pub extern "C" fn wr_storage_get(key: Value) -> Value {
    storage::storage_get(key)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_storage_set(key: Value, value: Value) -> Value {
    storage::storage_set(key, value)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_storage_delete(key: Value) -> Value {
    storage::storage_delete(key)
}

#[cfg(test)]
mod tests;
