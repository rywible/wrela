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

pub use value::{TypeId, Value};

use object::drop_object;

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
    float_box::box_float(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_unbox_float(val: Value) -> f64 {
    float_box::unbox_float(val)
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
        Some(num) => result::result_ok(float_box::box_float(num)),
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
                std::process::abort();
            }
        }
    }
    eprintln!("crash");
    std::process::abort();
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_actor_spawn(class_id: u32, instance: Value) -> Value {
    actor::actor_spawn(class_id, instance)
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
pub extern "C" fn wr_metrics_get(id: u32) -> u64 {
    metrics::get(id)
}

#[unsafe(no_mangle)]
pub extern "C" fn wr_metrics_reset() {
    metrics::reset()
}

#[cfg(test)]
mod tests;
