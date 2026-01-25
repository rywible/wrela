use crate::object::ObjHeader;
use crate::value::{header, TypeId, Value};
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
    let mut buffers: Vec<Vec<u8>> = Vec::with_capacity(parts.len());
    for part in parts {
        let bytes = value_to_bytes(*part);
        total = total.saturating_add(bytes.len());
        buffers.push(bytes);
    }
    let mut out = Vec::with_capacity(total);
    for buf in buffers {
        out.extend_from_slice(&buf);
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

fn value_to_bytes(val: Value) -> Vec<u8> {
    if val.is_ptr() {
        unsafe {
            let header = &*val.as_ptr();
            if header.type_id == TypeId::String as u32 {
                let s = &*(val.as_ptr() as *const StrObj);
                return s.bytes.clone();
            }
        }
    }
    if val.is_int() {
        return val.as_int().to_string().into_bytes();
    }
    if val.is_bool() {
        return if val.as_bool() { b"true".to_vec() } else { b"false".to_vec() };
    }
    if val.is_nil() {
        return b"nil".to_vec();
    }
    b"<obj>".to_vec()
}
