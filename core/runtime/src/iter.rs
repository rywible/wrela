use crate::list::as_list_ref;
use crate::map::{MapEntries, MapKey, as_map_ref, map_inline_entry, map_inline_len, map_is_inline, map_version};
use crate::object::ObjHeader;
use crate::value::{TypeId, Value, header};
use crate::{wr_rc_dec, wr_rc_inc};
use std::collections::hash_map;

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
        map: Value,
        index: usize,
        iter: Option<hash_map::Iter<'static, MapKey, Value>>,
        version: u64,
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
        unsafe { wr_rc_inc(iterable) };
        let version = map_version(map);
        let iter = if map_is_inline(map) {
            None
        } else {
            let iter = unsafe {
                match &(*map).entries {
                    MapEntries::Heap(entries) => Some(entries.iter()),
                    _ => None,
                }
            };
            iter.map(|iter| unsafe {
                std::mem::transmute::<hash_map::Iter<'_, MapKey, Value>, hash_map::Iter<'static, MapKey, Value>>(iter)
            })
        };
        let obj = Box::new(IterObj {
            header: header(TypeId::Iterator),
            kind: IterKind::Map {
                map: iterable,
                index: 0,
                iter,
                version,
            },
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
            IterKind::Map { map, index, iter, version } => {
                let map_ptr = match as_map_ref(*map) {
                    Some(map_ptr) => map_ptr,
                    None => return,
                };
                if map_version(map_ptr) != *version {
                    return;
                }
                if map_is_inline(map_ptr) {
                    let len = map_inline_len(map_ptr);
                    if *index < len {
                        if let Some((key, _)) = map_inline_entry(map_ptr, *index) {
                            wr_rc_inc(key.0);
                            *index += 1;
                            *dst_value = key.0;
                            *dst_done = Value::from_bool(false);
                            return;
                        }
                    }
                } else if let Some(iter) = iter.as_mut() {
                    if let Some((key, _)) = iter.next() {
                        wr_rc_inc(key.0);
                        *dst_value = key.0;
                        *dst_done = Value::from_bool(false);
                        return;
                    }
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
            IterKind::Map { map, .. } => {
                wr_rc_dec(*map);
            }
        }
        drop(Box::from_raw(iter));
    }
}
