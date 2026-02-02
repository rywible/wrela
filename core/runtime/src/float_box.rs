#![allow(dead_code)]

use crate::object::ObjHeader;
use crate::value::{TypeId, Value, header};

#[repr(C)]
struct FloatBox {
    header: ObjHeader,
    val: f64,
}

pub fn box_float(val: f64) -> Value {
    let boxed = Box::new(FloatBox {
        header: header(TypeId::Float),
        val,
    });
    Value::from_ptr(Box::into_raw(boxed) as *mut ObjHeader)
}

pub fn unbox_float(val: Value) -> f64 {
    if !val.is_ptr() {
        return 0.0;
    }
    unsafe {
        let header = &*val.as_ptr();
        if header.type_id != TypeId::Float as u32 {
            return 0.0;
        }
        let fb = val.as_ptr() as *mut FloatBox;
        (*fb).val
    }
}

pub unsafe fn drop_float_box(ptr: *mut ObjHeader) {
    let fb = ptr as *mut FloatBox;
    unsafe {
        drop(Box::from_raw(fb));
    }
}
