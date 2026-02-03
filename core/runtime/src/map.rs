use crate::object::ObjHeader;
use crate::value::{TypeId, Value, header, int_value, value_eq, value_hash as hash_value};
use crate::{wr_rc_dec, wr_rc_inc};
#[cfg(feature = "metrics")]
use crate::metrics::inc_alloc_map;
use crate::arena;
use std::collections::HashMap;
use std::collections::hash_map::{Entry, Iter as HashIter};
use std::hash::{Hash, Hasher};
use std::mem::MaybeUninit;

#[repr(C)]
pub struct MapObj {
    pub(crate) header: ObjHeader,
    pub(crate) entries: MapEntries,
    pub(crate) version: u64,
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

const INLINE_CAP: usize = 8;

pub(crate) enum MapEntries {
    Inline {
        len: usize,
        entries: [MaybeUninit<(MapKey, Value)>; INLINE_CAP],
    },
    Heap(HashMap<MapKey, Value>),
}

pub(crate) enum MapIter<'a> {
    Inline {
        entries: &'a [MaybeUninit<(MapKey, Value)>; INLINE_CAP],
        index: usize,
        len: usize,
    },
    Heap(HashIter<'a, MapKey, Value>),
}

impl<'a> MapIter<'a> {
    pub(crate) fn next(&mut self) -> Option<(MapKey, Value)> {
        match self {
            MapIter::Inline { entries, index, len } => {
                if *index >= *len {
                    return None;
                }
                let entry = unsafe { entries[*index].assume_init_ref() };
                *index += 1;
                Some(*entry)
            }
            MapIter::Heap(iter) => iter.next().map(|(k, v)| (*k, *v)),
        }
    }
}

impl MapEntries {
    fn new_inline() -> Self {
        let entries = unsafe { MaybeUninit::uninit().assume_init() };
        Self::Inline { len: 0, entries }
    }

    fn is_inline(&self) -> bool {
        matches!(self, Self::Inline { .. })
    }

    fn inline_len(&self) -> usize {
        match self {
            Self::Inline { len, .. } => *len,
            _ => 0,
        }
    }

    fn inline_entry(&self, index: usize) -> Option<(MapKey, Value)> {
        match self {
            Self::Inline { len, entries } if index < *len => {
                let entry = unsafe { entries[index].assume_init_ref() };
                Some(*entry)
            }
            _ => None,
        }
    }

    fn promote(&mut self) -> &mut HashMap<MapKey, Value> {
        let entries = std::mem::replace(self, MapEntries::new_inline());
        if let MapEntries::Inline { len, entries } = entries {
            let mut map = HashMap::with_capacity(len + 1);
            for idx in 0..len {
                let (key, val) = unsafe { entries[idx].assume_init_read() };
                map.insert(key, val);
            }
            *self = MapEntries::Heap(map);
        }
        match self {
            MapEntries::Heap(map) => map,
            MapEntries::Inline { .. } => unreachable!("promote should yield heap"),
        }
    }
}

pub fn map_new() -> Value {
    let obj = Box::new(MapObj {
        header: header(TypeId::Map),
        entries: MapEntries::new_inline(),
        version: 0,
    });
    #[cfg(feature = "metrics")]
    inc_alloc_map();
    Value::from_ptr(Box::into_raw(obj) as *mut ObjHeader)
}

pub fn map_new_local() -> Value {
    let obj = MapObj {
        header: header(TypeId::Map),
        entries: MapEntries::new_inline(),
        version: 0,
    };
    if let Some(ptr) = arena::alloc_in_current(obj) {
        #[cfg(feature = "metrics")]
        inc_alloc_map();
        return Value::from_ptr(ptr as *mut ObjHeader);
    }
    map_new()
}

pub fn map_get(map_val: Value, key: Value) -> Value {
    let map = match as_map_ref(map_val) {
        Some(map) => map,
        None => return Value::nil(),
    };
    unsafe {
        match &(*map).entries {
            MapEntries::Inline { len, entries } => {
                for idx in 0..*len {
                    let (stored_key, val) = entries[idx].assume_init_ref();
                    if *stored_key == MapKey(key) {
                        wr_rc_inc(*val);
                        return *val;
                    }
                }
            }
            MapEntries::Heap(entries) => {
                if let Some(val) = entries.get(&MapKey(key)).copied() {
                    wr_rc_inc(val);
                    return val;
                }
            }
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
        let entries_ptr = &mut (*map).entries as *mut MapEntries;
        if let MapEntries::Inline { len, entries } = &mut *entries_ptr {
            for idx in 0..*len {
                let (stored_key, stored_val) = entries[idx].assume_init_mut();
                if *stored_key == MapKey(key_val) {
                    let old = *stored_val;
                    *stored_val = val;
                    wr_rc_inc(val);
                    wr_rc_dec(old);
                    (*map).version = (*map).version.wrapping_add(1);
                    return;
                }
            }
            if *len < INLINE_CAP {
                entries[*len].write((MapKey(key_val), val));
                *len += 1;
                wr_rc_inc(key_val);
                wr_rc_inc(val);
                (*map).version = (*map).version.wrapping_add(1);
                return;
            }
        }
        if let MapEntries::Inline { .. } = &mut *entries_ptr {
            let map_entries = (&mut *entries_ptr).promote();
            map_entries.insert(MapKey(key_val), val);
            wr_rc_inc(key_val);
            wr_rc_inc(val);
            (*map).version = (*map).version.wrapping_add(1);
            return;
        }
        if let MapEntries::Heap(entries) = &mut *entries_ptr {
            match entries.entry(MapKey(key_val)) {
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
            (*map).version = (*map).version.wrapping_add(1);
        }
    }
}

pub fn drop_map(ptr: *mut ObjHeader) {
    let map = ptr as *mut MapObj;
    unsafe {
        match &mut (*map).entries {
            MapEntries::Inline { len, entries } => {
                for idx in 0..*len {
                    let (key, val) = entries[idx].assume_init_ref();
                    wr_rc_dec(key.0);
                    wr_rc_dec(*val);
                }
            }
            MapEntries::Heap(entries) => {
                for (key, val) in entries.iter() {
                    wr_rc_dec(key.0);
                    wr_rc_dec(*val);
                }
            }
        }
        drop(Box::from_raw(map));
    }
}

pub fn drop_map_in_arena(ptr: *mut ObjHeader) {
    let map = ptr as *mut MapObj;
    unsafe {
        match &mut (*map).entries {
            MapEntries::Inline { len, entries } => {
                for idx in 0..*len {
                    let (key, val) = entries[idx].assume_init_ref();
                    wr_rc_dec(key.0);
                    wr_rc_dec(*val);
                }
            }
            MapEntries::Heap(entries) => {
                for (key, val) in entries.iter() {
                    wr_rc_dec(key.0);
                    wr_rc_dec(*val);
                }
            }
        }
        std::ptr::drop_in_place(&mut (*map).entries);
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

pub(crate) fn map_len(map: *mut MapObj) -> usize {
    unsafe {
        match &(*map).entries {
            MapEntries::Inline { len, .. } => *len,
            MapEntries::Heap(entries) => entries.len(),
        }
    }
}

pub(crate) fn map_iter(map: *mut MapObj) -> MapIter<'static> {
    unsafe {
        match &(*map).entries {
            MapEntries::Inline { len, entries } => MapIter::Inline {
                entries,
                index: 0,
                len: *len,
            },
            MapEntries::Heap(entries) => {
                let iter = entries.iter();
                MapIter::Heap(std::mem::transmute::<HashIter<'_, MapKey, Value>, HashIter<'static, MapKey, Value>>(iter))
            }
        }
    }
}

pub(crate) fn map_get_raw(map: *mut MapObj, key: MapKey) -> Option<Value> {
    unsafe {
        match &(*map).entries {
            MapEntries::Inline { len, entries } => {
                for idx in 0..*len {
                    let (stored_key, val) = entries[idx].assume_init_ref();
                    if *stored_key == key {
                        return Some(*val);
                    }
                }
                None
            }
            MapEntries::Heap(entries) => entries.get(&key).copied(),
        }
    }
}

pub(crate) fn map_is_inline(map: *mut MapObj) -> bool {
    unsafe { (*map).entries.is_inline() }
}

pub(crate) fn map_inline_len(map: *mut MapObj) -> usize {
    unsafe { (*map).entries.inline_len() }
}

pub(crate) fn map_inline_entry(map: *mut MapObj, index: usize) -> Option<(MapKey, Value)> {
    unsafe { (*map).entries.inline_entry(index) }
}

pub(crate) fn map_version(map: *mut MapObj) -> u64 {
    unsafe { (*map).version }
}

fn is_valid_key(val: Value) -> bool {
    if int_value(val).is_some() || val.is_bool() || val.is_nil() {
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
