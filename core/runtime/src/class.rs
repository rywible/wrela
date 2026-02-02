use crate::object::ObjHeader;
use crate::value::{TypeId, Value, header_raw};
use crate::{wr_rc_dec, wr_rc_inc};
use std::collections::HashMap;

#[repr(C)]
pub struct ClassObj {
    header: ObjHeader,
    fields: HashMap<Vec<u8>, Value>,
}

pub fn class_new(
    class_id: u32,
    names_ptr: *const *const u8,
    lens_ptr: *const usize,
    count: usize,
) -> Value {
    if (names_ptr.is_null() || lens_ptr.is_null()) && count != 0 {
        return Value::nil();
    }
    let mut fields = HashMap::new();
    for i in 0..count {
        let name_ptr = unsafe { *names_ptr.add(i) };
        let len = unsafe { *lens_ptr.add(i) };
        if name_ptr.is_null() && len != 0 {
            continue;
        }
        let bytes = unsafe { std::slice::from_raw_parts(name_ptr, len) };
        fields.insert(bytes.to_vec(), Value::nil());
    }
    let obj = Box::new(ClassObj {
        header: header_raw(class_id.max(TypeId::UserBase as u32)),
        fields,
    });
    Value::from_ptr(Box::into_raw(obj) as *mut ObjHeader)
}

pub fn class_get(obj_val: Value, name_ptr: *const u8, len: usize) -> Value {
    let obj = match as_class(obj_val) {
        Some(obj) => obj,
        None => return Value::nil(),
    };
    if name_ptr.is_null() && len != 0 {
        return Value::nil();
    }
    let key = unsafe { std::slice::from_raw_parts(name_ptr, len) };
    unsafe {
        if let Some(val) = (*obj).fields.get(key).copied() {
            wr_rc_inc(val);
            return val;
        }
    }
    Value::nil()
}

pub fn class_set(obj_val: Value, name_ptr: *const u8, len: usize, val: Value) {
    let obj = match as_class(obj_val) {
        Some(obj) => obj,
        None => return,
    };
    if name_ptr.is_null() && len != 0 {
        return;
    }
    let key = unsafe { std::slice::from_raw_parts(name_ptr, len) };
    unsafe {
        if let Some(old) = (*obj).fields.insert(key.to_vec(), val) {
            wr_rc_dec(old);
        }
        wr_rc_inc(val);
    }
}

pub unsafe fn drop_class(ptr: *mut ObjHeader) {
    let obj = ptr as *mut ClassObj;
    unsafe {
        for (_key, val) in (*obj).fields.iter() {
            wr_rc_dec(*val);
        }
        drop(Box::from_raw(obj));
    }
}

fn as_class(val: Value) -> Option<*mut ClassObj> {
    if !val.is_ptr() {
        return None;
    }
    unsafe {
        let header = &*val.as_ptr();
        if header.type_id < TypeId::UserBase as u32 {
            return None;
        }
    }
    Some(val.as_ptr() as *mut ClassObj)
}
