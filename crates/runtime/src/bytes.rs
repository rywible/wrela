use crate::list;
use crate::object::ObjHeader;
use crate::string;
use crate::value::{TypeId, Value, header, int_value};

#[repr(C)]
pub struct BytesObj {
    header: ObjHeader,
    bytes: Vec<u8>,
}

pub fn bytes_from_slice(bytes: &[u8]) -> Value {
    let obj = Box::new(BytesObj {
        header: header(TypeId::Bytes),
        bytes: bytes.to_vec(),
    });
    Value::from_ptr(Box::into_raw(obj) as *mut ObjHeader)
}

pub fn bytes_from_string(val: Value) -> Value {
    string::with_string_bytes(val, |bytes| bytes_from_slice(bytes)).unwrap_or(Value::nil())
}

pub fn bytes_to_string(val: Value) -> Value {
    with_bytes(val, |bytes| match std::str::from_utf8(bytes) {
        Ok(_) => string::str_from_bytes(bytes),
        Err(_) => Value::nil(),
    })
    .unwrap_or(Value::nil())
}

pub fn bytes_len(val: Value) -> Value {
    with_bytes(val, |bytes| Value::from_int(bytes.len() as i64)).unwrap_or(Value::nil())
}

#[allow(dead_code)]
pub fn bytes_from_list(list_val: Value) -> Value {
    let list = match list::as_list_ref(list_val) {
        Some(list) => list,
        None => return Value::nil(),
    };
    let len = unsafe { (*list).len };
    let mut out = Vec::with_capacity(len);
    unsafe {
        for val in (*list).data.iter().take(len) {
            let Some(i) = int_value(*val) else {
                return Value::nil();
            };
            if !(0..=255).contains(&i) {
                return Value::nil();
            }
            out.push(i as u8);
        }
    }
    bytes_from_slice(&out)
}

#[allow(dead_code)]
pub fn bytes_to_list(bytes_val: Value) -> Value {
    let bytes = match with_bytes(bytes_val, |bytes| bytes.to_vec()) {
        Some(bytes) => bytes,
        None => return Value::nil(),
    };
    let list = list::list_new(bytes.len());
    for (idx, byte) in bytes.iter().enumerate() {
        list::list_set(list, idx, Value::from_int(*byte as i64));
    }
    list
}

pub fn drop_bytes(ptr: *mut ObjHeader) {
    let bytes = ptr as *mut BytesObj;
    unsafe {
        drop(Box::from_raw(bytes));
    }
}

pub fn with_bytes<F, R>(val: Value, f: F) -> Option<R>
where
    F: FnOnce(&[u8]) -> R,
{
    if !val.is_ptr() {
        return None;
    }
    unsafe {
        let header = &*val.as_ptr();
        if header.type_id != TypeId::Bytes as u32 {
            return None;
        }
        let bytes = &*(val.as_ptr() as *const BytesObj);
        Some(f(&bytes.bytes))
    }
}

#[allow(dead_code)]
pub fn bytes_eq(a: Value, b: Value) -> Option<bool> {
    with_bytes(a, |ab| with_bytes(b, |bb| ab == bb)).and_then(|v| v)
}

#[allow(dead_code)]
pub fn bytes_hash<H: std::hash::Hasher>(val: Value, state: &mut H) -> bool {
    use std::hash::Hash;
    if let Some(bytes) = with_bytes(val, |bytes| bytes.to_vec()) {
        bytes.hash(state);
        return true;
    }
    false
}
