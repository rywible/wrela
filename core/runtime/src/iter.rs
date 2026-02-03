use crate::list::as_list_ref;
use crate::map::as_map_ref;
use crate::object::ObjHeader;
use crate::value::{TypeId, Value, header};
use crate::{wr_rc_dec, wr_rc_inc};

#[repr(C)]
pub struct IterObj {
    header: ObjHeader,
    kind: IterKind,
}

#[repr(C)]
pub enum IterKind {
    List {
        list: Value,
        index: usize,
    },
    Map {
        keys: Vec<Value>,
        index: usize,
    },
}

pub fn iter_init(iterable: Value) -> Value {
    if let Some(_list) = as_list_ref(iterable) {
        unsafe { wr_rc_inc(iterable) };
        let obj = Box::new(IterObj {
            header: header(TypeId::Iterator),
            kind: IterKind::List {
                list: iterable,
                index: 0,
            },
        });
        return Value::from_ptr(Box::into_raw(obj) as *mut ObjHeader);
    }
    if let Some(map) = as_map_ref(iterable) {
        let mut keys = Vec::new();
        unsafe {
            for (k, _) in (*map).entries.iter() {
                keys.push(k.0);
                wr_rc_inc(k.0);
            }
        }
        let obj = Box::new(IterObj {
            header: header(TypeId::Iterator),
            kind: IterKind::Map { keys, index: 0 },
        });
        return Value::from_ptr(Box::into_raw(obj) as *mut ObjHeader);
    }
    Value::nil()
}

pub fn iter_next(iter_val: Value, dst_value: *mut Value, dst_done: *mut Value) {
    if dst_value.is_null() || dst_done.is_null() {
        return;
    }
    unsafe {
        *dst_done = Value::from_bool(true);
        *dst_value = Value::nil();
    }
    if !iter_val.is_ptr() {
        return;
    }
    unsafe {
        let header = &*iter_val.as_ptr();
        if header.type_id != TypeId::Iterator as u32 {
            return;
        }
        let iter = &mut *(iter_val.as_ptr() as *mut IterObj);
        match &mut iter.kind {
            IterKind::List { list, index } => {
                if let Some(list_ptr) = as_list_ref(*list) {
                    if *index < (*list_ptr).len {
                        let val = (&(*list_ptr).data)[*index];
                        wr_rc_inc(val);
                        *index += 1;
                        *dst_value = val;
                        *dst_done = Value::from_bool(false);
                        return;
                    }
                }
            }
            IterKind::Map { keys, index } => {
                if *index < keys.len() {
                    let key = keys[*index];
                    wr_rc_inc(key);
                    *index += 1;
                    *dst_value = key;
                    *dst_done = Value::from_bool(false);
                    return;
                }
            }
        }
    }
}

pub unsafe fn drop_iter(ptr: *mut ObjHeader) {
    let iter = ptr as *mut IterObj;
    unsafe {
        match &mut (*iter).kind {
            IterKind::List { list, .. } => {
                wr_rc_dec(*list);
            }
            IterKind::Map { keys, .. } => {
                for key in keys.iter() {
                    wr_rc_dec(*key);
                }
            }
        }
        drop(Box::from_raw(iter));
    }
}
