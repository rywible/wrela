use crate::object::ObjHeader;
use crate::value::{header, value_eq, value_hash as hash_value, TypeId, Value};
use crate::{wr_rc_dec, wr_rc_inc};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::hash::{Hash, Hasher};

#[repr(C)]
pub struct MapObj {
    pub(crate) header: ObjHeader,
    pub(crate) entries: HashMap<MapKey, Value>,
}

#[derive(Clone, Copy)]
pub(crate) struct MapKey(pub(crate) Value);

impl PartialEq for MapKey {
    fn eq(&self, other: &Self) -> bool {
        value_eq(self.0, other.0)
    }
}

impl Eq for MapKey {}

impl Hash for MapKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_value(self.0, state);
    }
}

pub fn map_new() -> Value {
    let obj = Box::new(MapObj {
        header: header(TypeId::Map),
        entries: HashMap::new(),
    });
    Value::from_ptr(Box::into_raw(obj) as *mut ObjHeader)
}

pub fn map_get(map_val: Value, key: Value) -> Value {
    let map = match as_map_ref(map_val) {
        Some(map) => map,
        None => return Value::nil(),
    };
    unsafe {
        if let Some(val) = (*map).entries.get(&MapKey(key)).copied() {
            wr_rc_inc(val);
            return val;
        }
        Value::nil()
    }
}

pub fn map_set(map_val: Value, key: Value, val: Value) {
    let map = match as_map_ref(map_val) {
        Some(map) => map,
        None => return,
    };
    if !is_valid_key(key) {
        return;
    }
    unsafe {
        let key_val = key;
        match (*map).entries.entry(MapKey(key_val)) {
            Entry::Occupied(mut entry) => {
                let old = entry.insert(val);
                wr_rc_inc(val);
                wr_rc_dec(old);
            }
            Entry::Vacant(entry) => {
                entry.insert(val);
                wr_rc_inc(key_val);
                wr_rc_inc(val);
            }
        }
    }
}

pub fn drop_map(ptr: *mut ObjHeader) {
    let map = ptr as *mut MapObj;
    unsafe {
        for (key, val) in (*map).entries.iter() {
            wr_rc_dec(key.0);
            wr_rc_dec(*val);
        }
        drop(Box::from_raw(map));
    }
}

pub(crate) fn as_map_ref(val: Value) -> Option<*mut MapObj> {
    if !val.is_ptr() {
        return None;
    }
    unsafe {
        let header = &*val.as_ptr();
        if header.type_id != TypeId::Map as u32 {
            return None;
        }
    }
    Some(val.as_ptr() as *mut MapObj)
}

fn is_valid_key(val: Value) -> bool {
    if val.is_int() || val.is_bool() || val.is_nil() {
        return true;
    }
    if val.is_ptr() {
        unsafe {
            let header = &*val.as_ptr();
            return header.type_id == TypeId::String as u32;
        }
    }
    false
}
