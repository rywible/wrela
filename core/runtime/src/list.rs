use crate::object::ObjHeader;
use crate::value::{TypeId, Value, header};
use crate::{wr_rc_dec, wr_rc_inc};

const FLAG_MUTABLE: u32 = 1;

#[repr(C)]
pub struct ListObj {
    pub(crate) header: ObjHeader,
    pub(crate) len: usize,
    pub(crate) cap: usize,
    pub(crate) data: Vec<Value>,
    pub(crate) flags: u32,
}

pub fn list_new(len: usize) -> Value {
    let mut data = Vec::with_capacity(len);
    data.resize(len, Value::nil());
    let obj = Box::new(ListObj {
        header: header(TypeId::List),
        len,
        cap: len,
        data,
        flags: FLAG_MUTABLE,
    });
    Value::from_ptr(Box::into_raw(obj) as *mut ObjHeader)
}

pub fn list_get(list_val: Value, idx: usize) -> Value {
    let list = match as_list_ref(list_val) {
        Some(list) => list,
        None => return Value::nil(),
    };
    unsafe {
        if idx >= (*list).len {
            return Value::nil();
        }
        let val = (&(*list).data)[idx];
        wr_rc_inc(val);
        val
    }
}

pub fn list_set(list_val: Value, idx: usize, val: Value) {
    let list = match as_list_ref(list_val) {
        Some(list) => list,
        None => return,
    };
    unsafe {
        if (*list).flags & FLAG_MUTABLE == 0 {
            return;
        }
        if idx >= (*list).len {
            return;
        }
        let old = (&(*list).data)[idx];
        (&mut (*list).data)[idx] = val;
        wr_rc_inc(val);
        wr_rc_dec(old);
    }
}

pub fn list_push(list_val: Value, val: Value) {
    let list = match as_list_ref(list_val) {
        Some(list) => list,
        None => return,
    };
    unsafe {
        if (*list).flags & FLAG_MUTABLE == 0 {
            return;
        }
        (*list).data.push(val);
        (*list).len += 1;
        (*list).cap = (*list).data.capacity();
        wr_rc_inc(val);
    }
}

pub fn list_len(list_val: Value) -> Value {
    let list = match as_list_ref(list_val) {
        Some(list) => list,
        None => return Value::nil(),
    };
    unsafe { Value::from_int((*list).len as i64) }
}

pub fn drop_list(ptr: *mut ObjHeader) {
    let list = ptr as *mut ListObj;
    unsafe {
        for val in (*list).data.iter() {
            wr_rc_dec(*val);
        }
        drop(Box::from_raw(list));
    }
}

pub(crate) fn as_list_ref(val: Value) -> Option<*mut ListObj> {
    if !val.is_ptr() {
        return None;
    }
    unsafe {
        let header = &*val.as_ptr();
        if header.type_id != TypeId::List as u32 {
            return None;
        }
    }
    Some(val.as_ptr() as *mut ListObj)
}
