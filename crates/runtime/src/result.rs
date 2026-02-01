use crate::object::ObjHeader;
use crate::value::{TypeId, Value, header};
use crate::{wr_rc_dec, wr_rc_inc};

#[repr(C)]
struct ResultObj {
    header: ObjHeader,
    is_ok: u64,
    value: Value,
}

pub fn result_ok(value: Value) -> Value {
    unsafe { wr_rc_inc(value) };
    let obj = Box::new(ResultObj {
        header: header(TypeId::Result),
        is_ok: 1,
        value,
    });
    Value::from_ptr(Box::into_raw(obj) as *mut ObjHeader)
}

pub fn result_err(value: Value) -> Value {
    unsafe { wr_rc_inc(value) };
    let obj = Box::new(ResultObj {
        header: header(TypeId::Result),
        is_ok: 0,
        value,
    });
    Value::from_ptr(Box::into_raw(obj) as *mut ObjHeader)
}

pub fn result_is_ok(result: Value) -> Value {
    if !result.is_ptr() {
        return Value::from_bool(false);
    }
    unsafe {
        let header = &*result.as_ptr();
        if header.type_id != TypeId::Result as u32 {
            return Value::from_bool(false);
        }
        let obj = result.as_ptr() as *const ResultObj;
        Value::from_bool((*obj).is_ok != 0)
    }
}

pub fn result_unwrap(result: Value) -> Value {
    if !result.is_ptr() {
        return Value::nil();
    }
    unsafe {
        let header = &*result.as_ptr();
        if header.type_id != TypeId::Result as u32 {
            return Value::nil();
        }
        let obj = result.as_ptr() as *const ResultObj;
        let value = (*obj).value;
        wr_rc_inc(value);
        value
    }
}

pub fn result_parts(result: Value) -> Option<(bool, Value)> {
    if !result.is_ptr() {
        return None;
    }
    unsafe {
        let header = &*result.as_ptr();
        if header.type_id != TypeId::Result as u32 {
            return None;
        }
        let obj = result.as_ptr() as *const ResultObj;
        Some(((*obj).is_ok != 0, (*obj).value))
    }
}

pub unsafe fn drop_result(ptr: *mut ObjHeader) {
    let obj = unsafe { Box::from_raw(ptr as *mut ResultObj) };
    unsafe { wr_rc_dec(obj.value) };
}
