use crate::object::ObjHeader;
use crate::value::{TypeId, Value, header, int_value};
use crate::{wr_rc_dec, wr_rc_inc};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

#[repr(C)]
pub struct StrObj {
    header: ObjHeader,
    bytes: Vec<u8>,
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
    });
    Value::from_ptr(Box::into_raw(s) as *mut ObjHeader)
}

pub fn str_from_bytes(bytes: &[u8]) -> Value {
    let s = Box::new(StrObj {
        header: header(TypeId::String),
        bytes: bytes.to_vec(),
    });
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
    });
    Value::from_ptr(Box::into_raw(s) as *mut ObjHeader)
}

pub fn drop_string(ptr: *mut ObjHeader) {
    let s = ptr as *mut StrObj;
    unsafe {
        drop(Box::from_raw(s));
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
    if val.is_bool() {
        return if val.as_bool() { 4 } else { 5 };
    }
    if val.is_nil() {
        return 3;
    }
    5
}

fn write_value_bytes(val: Value, out: &mut Vec<u8>) {
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
