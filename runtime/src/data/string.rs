use crate::arena;
#[cfg(feature = "metrics")]
use crate::metrics::inc_alloc_string;
use crate::object::ObjHeader;
use crate::value::{TypeId, Value, header, int_value};
use crate::{wr_rc_dec, wr_rc_inc};
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

#[repr(C)]
pub struct StrObj {
    header: ObjHeader,
    bytes: Vec<u8>,
    arena_backed: bool,
}

static INTERN: OnceLock<Mutex<HashMap<Vec<u8>, Value>>> = OnceLock::new();

fn intern_map() -> &'static Mutex<HashMap<Vec<u8>, Value>> {
    INTERN.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn str_from_utf8(ptr: *const u8, len: usize) -> Value {
    if ptr.is_null() && len != 0 {
        return Value::nil();
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    if std::str::from_utf8(bytes).is_err() {
        return Value::nil();
    }
    let s = Box::new(StrObj {
        header: header(TypeId::String),
        bytes: bytes.to_vec(),
        arena_backed: false,
    });
    #[cfg(feature = "metrics")]
    inc_alloc_string();
    Value::from_ptr(Box::into_raw(s) as *mut ObjHeader)
}

// Intern a UTF-8 string literal from static bytes without allocating a temporary string first.
//
// This is designed for compiler-emitted string literals that are treated as constants:
// callers typically do not participate in RC for the returned Value.
pub fn str_intern_utf8(ptr: *const u8, len: usize) -> Value {
    if ptr.is_null() && len != 0 {
        return Value::nil();
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    if std::str::from_utf8(bytes).is_err() {
        return Value::nil();
    }
    let mut map = intern_map().lock().expect("intern map lock");
    if let Some(existing) = map.get(bytes) {
        // Return an owned reference for the caller while keeping the map's anchor ref.
        unsafe { wr_rc_inc(*existing) };
        return *existing;
    }
    let s = Box::new(StrObj {
        header: header(TypeId::String),
        bytes: bytes.to_vec(),
        arena_backed: false,
    });
    #[cfg(feature = "metrics")]
    inc_alloc_string();
    let val = Value::from_ptr(Box::into_raw(s) as *mut ObjHeader);
    // Keep one ref alive for the intern table.
    map.insert(bytes.to_vec(), val);
    unsafe { wr_rc_inc(val) };
    val
}

pub fn str_from_bytes(bytes: &[u8]) -> Value {
    let s = Box::new(StrObj {
        header: header(TypeId::String),
        bytes: bytes.to_vec(),
        arena_backed: false,
    });
    #[cfg(feature = "metrics")]
    inc_alloc_string();
    Value::from_ptr(Box::into_raw(s) as *mut ObjHeader)
}

pub fn str_intern(val: Value) -> Value {
    if !val.is_ptr() {
        return val;
    }
    unsafe {
        let header = &*val.as_ptr();
        if header.type_id != TypeId::String as u32 {
            return val;
        }
    }
    let bytes = unsafe { &*(val.as_ptr() as *mut StrObj) }.bytes.clone();
    let mut map = intern_map().lock().expect("intern map lock");
    if let Some(existing) = map.get(&bytes) {
        unsafe { wr_rc_dec(val) };
        // Return an owned reference for the caller while keeping the map's anchor ref.
        unsafe { wr_rc_inc(*existing) };
        return *existing;
    }
    map.insert(bytes, val);
    unsafe { wr_rc_inc(val) };
    val
}

pub fn str_concat(parts_ptr: *const Value, parts_len: usize) -> Value {
    if parts_ptr.is_null() && parts_len != 0 {
        return Value::nil();
    }
    let parts = unsafe { std::slice::from_raw_parts(parts_ptr, parts_len) };
    let mut total = 0usize;
    for part in parts {
        total = total.saturating_add(value_bytes_len(*part));
    }
    let mut out = Vec::with_capacity(total);
    for part in parts {
        write_value_bytes(*part, &mut out);
    }
    let s = Box::new(StrObj {
        header: header(TypeId::String),
        bytes: out,
        arena_backed: false,
    });
    #[cfg(feature = "metrics")]
    inc_alloc_string();
    Value::from_ptr(Box::into_raw(s) as *mut ObjHeader)
}

pub fn str_concat_local(parts_ptr: *const Value, parts_len: usize) -> Value {
    if parts_ptr.is_null() && parts_len != 0 {
        return Value::nil();
    }
    let parts = unsafe { std::slice::from_raw_parts(parts_ptr, parts_len) };
    let mut total = 0usize;
    for part in parts {
        total = total.saturating_add(value_bytes_len(*part));
    }
    if let Some(bytes_ptr) = arena::alloc_bytes_in_current(total, 1) {
        unsafe {
            let mut out = Vec::from_raw_parts(bytes_ptr, 0, total);
            for part in parts {
                write_value_bytes(*part, &mut out);
            }
            let obj = StrObj {
                header: header(TypeId::String),
                bytes: out,
                arena_backed: true,
            };
            if let Some(ptr) = arena::alloc_in_current(obj) {
                #[cfg(feature = "metrics")]
                inc_alloc_string();
                return Value::from_ptr(ptr as *mut ObjHeader);
            }
        }
    }
    str_concat(parts_ptr, parts_len)
}

pub fn drop_string(ptr: *mut ObjHeader) {
    let s = ptr as *mut StrObj;
    unsafe {
        drop(Box::from_raw(s));
    }
}

pub fn drop_string_in_arena(ptr: *mut ObjHeader) {
    let s = ptr as *mut StrObj;
    unsafe {
        if !(*s).arena_backed {
            std::ptr::drop_in_place(&mut (*s).bytes);
        }
    }
}

pub fn with_string_bytes<F, R>(val: Value, f: F) -> Option<R>
where
    F: FnOnce(&[u8]) -> R,
{
    if !val.is_ptr() {
        return None;
    }
    unsafe {
        let header = &*val.as_ptr();
        if header.type_id != TypeId::String as u32 {
            return None;
        }
        let s = &*(val.as_ptr() as *const StrObj);
        Some(f(&s.bytes))
    }
}

fn value_bytes_len(val: Value) -> usize {
    if val.is_ptr() {
        unsafe {
            let header = &*val.as_ptr();
            if header.type_id == TypeId::String as u32 {
                let s = &*(val.as_ptr() as *const StrObj);
                return s.bytes.len();
            }
        }
    }
    if let Some(i) = int_value(val) {
        return int_len(i);
    }
    if val.is_float() {
        return val.as_float().to_string().len();
    }
    if val.is_bool() {
        return if val.as_bool() { 4 } else { 5 };
    }
    if val.is_nil() {
        return 7; // b"nothing".len()
    }
    render_value_bytes(val).len()
}

fn write_value_bytes(val: Value, out: &mut Vec<u8>) {
    let mut seen = HashSet::new();
    write_value_bytes_recursive(val, out, 0, &mut seen);
}

fn render_value_bytes(val: Value) -> Vec<u8> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    write_value_bytes_recursive(val, &mut out, 0, &mut seen);
    out
}

fn write_value_bytes_recursive(
    val: Value,
    out: &mut Vec<u8>,
    depth: usize,
    seen: &mut HashSet<usize>,
) {
    if depth > 64 {
        out.extend_from_slice(b"<depth>");
        return;
    }
    if val.is_ptr() {
        unsafe {
            let header = &*val.as_ptr();
            if header.type_id == TypeId::String as u32 {
                let s = &*(val.as_ptr() as *const StrObj);
                out.extend_from_slice(&s.bytes);
                return;
            }
        }
    }
    if let Some(i) = int_value(val) {
        write_int(out, i);
        return;
    }
    if val.is_float() {
        out.extend_from_slice(val.as_float().to_string().as_bytes());
        return;
    }
    if val.is_bool() {
        if val.as_bool() {
            out.extend_from_slice(b"true");
        } else {
            out.extend_from_slice(b"false");
        }
        return;
    }
    if val.is_nil() {
        out.extend_from_slice(b"nothing");
        return;
    }
    if !val.is_ptr() {
        out.extend_from_slice(b"<value>");
        return;
    }

    let ptr = val.as_ptr() as usize;
    if !seen.insert(ptr) {
        out.extend_from_slice(b"<cycle>");
        return;
    }

    unsafe {
        let header = &*val.as_ptr();
        if header.type_id == TypeId::List as u32 {
            if let Some(list) = crate::list::as_list_ref(val) {
                out.push(b'[');
                for idx in 0..(*list).len {
                    if idx > 0 {
                        out.extend_from_slice(b", ");
                    }
                    let item = (&(*list).data)[idx];
                    write_value_bytes_recursive(item, out, depth + 1, seen);
                }
                out.push(b']');
            } else {
                out.extend_from_slice(b"<list>");
            }
            seen.remove(&ptr);
            return;
        }
        if header.type_id == TypeId::Map as u32 {
            if let Some(map) = crate::map::as_map_ref(val) {
                let mut entries: Vec<(Vec<u8>, Value)> =
                    Vec::with_capacity(crate::map::map_len(map));
                let mut iter = crate::map::map_iter(map);
                while let Some((map_key, map_value)) = iter.next() {
                    let mut key_render = Vec::new();
                    write_value_bytes_recursive(map_key.0, &mut key_render, depth + 1, seen);
                    entries.push((key_render, map_value));
                }
                entries.sort_by(|left, right| left.0.cmp(&right.0));
                out.push(b'{');
                for (index, (key_render, value_render)) in entries.into_iter().enumerate() {
                    if index > 0 {
                        out.extend_from_slice(b", ");
                    }
                    out.extend_from_slice(&key_render);
                    out.extend_from_slice(b": ");
                    write_value_bytes_recursive(value_render, out, depth + 1, seen);
                }
                out.push(b'}');
            } else {
                out.extend_from_slice(b"<map>");
            }
            seen.remove(&ptr);
            return;
        }
        if header.type_id == TypeId::Result as u32 {
            if let Some((is_ok, result_value)) = crate::result::result_parts(val) {
                if is_ok {
                    out.extend_from_slice(b"Ok(");
                } else {
                    out.extend_from_slice(b"Err(");
                }
                write_value_bytes_recursive(result_value, out, depth + 1, seen);
                out.push(b')');
            } else {
                out.extend_from_slice(b"<result>");
            }
            seen.remove(&ptr);
            return;
        }
        if header.type_id >= TypeId::UserBase as u32 {
            if let Some((type_id, fields)) = crate::class::class_type_and_fields(val) {
                out.extend_from_slice(b"Class#");
                write_int(out, type_id as i64);
                out.push(b'{');
                for (index, (name, field_value)) in fields.into_iter().enumerate() {
                    if index > 0 {
                        out.extend_from_slice(b", ");
                    }
                    out.extend_from_slice(&name);
                    out.extend_from_slice(b": ");
                    write_value_bytes_recursive(field_value, out, depth + 1, seen);
                }
                out.push(b'}');
            } else {
                out.extend_from_slice(b"<class>");
            }
            seen.remove(&ptr);
            return;
        }
    }

    seen.remove(&ptr);
    out.extend_from_slice(b"<obj>");
}

fn int_len(mut val: i64) -> usize {
    if val == 0 {
        return 1;
    }
    if val == i64::MIN {
        return 20;
    }
    let mut len = 0usize;
    if val < 0 {
        len += 1;
        val = -val;
    }
    while val > 0 {
        len += 1;
        val /= 10;
    }
    len
}

fn write_int(out: &mut Vec<u8>, val: i64) {
    if val == 0 {
        out.push(b'0');
        return;
    }
    if val == i64::MIN {
        out.extend_from_slice(b"-9223372036854775808");
        return;
    }
    let mut n = val;
    if n < 0 {
        out.push(b'-');
        n = -n;
    }
    let mut buf = [0u8; 20];
    let mut i = 0usize;
    while n > 0 {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    for idx in (0..i).rev() {
        out.push(buf[idx]);
    }
}
