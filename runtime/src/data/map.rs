use crate::arena;
#[cfg(feature = "metrics")]
use crate::metrics::inc_alloc_map;
use crate::object::ObjHeader;
use crate::value::{TypeId, Value, header, int_value, value_eq, value_hash as hash_value};
use crate::{wr_rc_dec, wr_rc_inc};
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::hash_map::{Entry, Iter as HashIter};
use std::hash::BuildHasherDefault;
use std::hash::{Hash, Hasher};
use std::mem::MaybeUninit;
#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};

// Default `std::collections::HashMap` uses SipHash, which is great for adversarial keys and
// terrible for perf macrobenches. Wrela's map is a runtime primitive; for now we bias hard
// toward throughput and accept that this is non-cryptographic.
//
// This is intentionally simple and fast (fxhash-style).
#[derive(Default)]
pub(crate) struct FastHasher {
    hash: u64,
}

impl Hasher for FastHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        // FNV-ish: cheap and decent for small keys.
        let mut h = self.hash;
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(0x100_0000_01b3);
        }
        self.hash = h;
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        // Mix 64-bit chunks with a single multiply; good enough for ints/pointers.
        let mut h = self.hash ^ i;
        h = h.wrapping_mul(0x517c_c1b7_2722_0a95);
        self.hash = h;
    }

    #[inline]
    fn write_i64(&mut self, i: i64) {
        self.write_u64(i as u64);
    }

    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.write_u64(i as u64);
    }
}

type FastBuildHasher = BuildHasherDefault<FastHasher>;
type FastHashMap<K, V> = HashMap<K, V, FastBuildHasher>;

#[inline]
unsafe fn rc_inc_if_managed(val: Value) {
    if val.is_ptr() && !arena::is_arena_value(val) {
        unsafe { wr_rc_inc(val) };
    }
}

#[inline]
unsafe fn rc_dec_if_managed(val: Value) {
    if val.is_ptr() && !arena::is_arena_value(val) {
        unsafe { wr_rc_dec(val) };
    }
}

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
const CACHE_SHAPE_INLINE_SLOT: u8 = 1;
const CACHE_SHAPE_HEAP_KEY: u8 = 2;
const CACHE_SHAPE_MISSING: u8 = 3;

#[derive(Clone, Copy, Default)]
struct MapInlineCache {
    map_ptr: usize,
    version: u64,
    key: Value,
    shape: u8,
    inline_index: usize,
}

impl MapInlineCache {
    #[cfg(test)]
    fn clear(&mut self) {
        *self = Self::default();
    }

    fn store_inline_slot(&mut self, map: *mut MapObj, version: u64, key: Value, index: usize) {
        self.map_ptr = map as usize;
        self.version = version;
        self.key = key;
        self.shape = CACHE_SHAPE_INLINE_SLOT;
        self.inline_index = index;
    }

    fn store_heap_key(&mut self, map: *mut MapObj, version: u64, key: Value) {
        self.map_ptr = map as usize;
        self.version = version;
        self.key = key;
        self.shape = CACHE_SHAPE_HEAP_KEY;
        self.inline_index = 0;
    }

    fn store_missing(&mut self, map: *mut MapObj, version: u64, key: Value) {
        self.map_ptr = map as usize;
        self.version = version;
        self.key = key;
        self.shape = CACHE_SHAPE_MISSING;
        self.inline_index = 0;
    }

    fn matches(&self, map: *mut MapObj, version: u64, key: Value) -> bool {
        self.shape != 0
            && self.map_ptr == map as usize
            && self.version == version
            && value_eq(self.key, key)
    }
}

thread_local! {
    static MAP_INLINE_CACHE: RefCell<MapInlineCache> = RefCell::new(MapInlineCache::default());
}

#[cfg(test)]
static MAP_IC_HITS: AtomicU64 = AtomicU64::new(0);
#[cfg(test)]
static MAP_IC_MISSES: AtomicU64 = AtomicU64::new(0);

pub(crate) enum MapEntries {
    Inline {
        len: usize,
        entries: [MaybeUninit<(MapKey, Value)>; INLINE_CAP],
    },
    HeapGeneric(FastHashMap<MapKey, Value>),
    // Fast path for the common hot case: immediate integer keys.
    HeapInt(IntMap),
}

pub(crate) enum MapIter<'a> {
    Inline {
        entries: &'a [MaybeUninit<(MapKey, Value)>; INLINE_CAP],
        index: usize,
        len: usize,
    },
    HeapGeneric(HashIter<'a, MapKey, Value>),
    HeapInt(IntMapIter<'a>),
}

#[derive(Clone, Copy, Default)]
struct IntSlot {
    key: i64,
    val: Value,
    full: bool,
}

// Minimal open-addressing table for immediate integer keys.
// No delete support (language maps don't expose deletion today), so we can keep it simple.
pub(crate) struct IntMap {
    len: usize,
    mask: usize,
    slots: Vec<IntSlot>,
}

impl IntMap {
    fn new(cap: usize) -> Self {
        let cap = cap.max(16).next_power_of_two();
        Self {
            len: 0,
            mask: cap - 1,
            slots: vec![IntSlot::default(); cap],
        }
    }

    #[inline(always)]
    fn len(&self) -> usize {
        self.len
    }

    #[inline(always)]
    fn hash(key: i64) -> usize {
        // A simple mix that's good enough for consecutive integer keys.
        let mut x = key as u64;
        x ^= x >> 33;
        x = x.wrapping_mul(0xff51afd7ed558ccd);
        x ^= x >> 33;
        x = x.wrapping_mul(0xc4ceb9fe1a85ec53);
        x ^= x >> 33;
        x as usize
    }

    fn load_factor_exceeded(&self) -> bool {
        // Keep probing short: ~70% load.
        self.len * 10 >= self.slots.len() * 7
    }

    fn get(&self, key: i64) -> Option<Value> {
        let mut idx = Self::hash(key) & self.mask;
        loop {
            let slot = unsafe { self.slots.get_unchecked(idx) };
            if !slot.full {
                return None;
            }
            if slot.key == key {
                return Some(slot.val);
            }
            idx = (idx + 1) & self.mask;
        }
    }

    fn insert(&mut self, key: i64, val: Value) -> Option<Value> {
        if self.load_factor_exceeded() {
            self.grow();
        }
        let mut idx = Self::hash(key) & self.mask;
        loop {
            let slot = unsafe { self.slots.get_unchecked_mut(idx) };
            if !slot.full {
                slot.full = true;
                slot.key = key;
                slot.val = val;
                self.len += 1;
                return None;
            }
            if slot.key == key {
                let old = slot.val;
                slot.val = val;
                return Some(old);
            }
            idx = (idx + 1) & self.mask;
        }
    }

    fn grow(&mut self) {
        let mut next = IntMap::new(self.slots.len() * 2);
        for slot in self.slots.iter_mut() {
            if slot.full {
                let key = slot.key;
                let val = slot.val;
                let _ = next.insert(key, val);
            }
        }
        *self = next;
    }

    fn iter(&self) -> IntMapIter<'_> {
        IntMapIter { map: self, idx: 0 }
    }
}

pub(crate) struct IntMapIter<'a> {
    map: &'a IntMap,
    idx: usize,
}

impl<'a> Iterator for IntMapIter<'a> {
    type Item = (i64, Value);

    fn next(&mut self) -> Option<Self::Item> {
        while self.idx < self.map.slots.len() {
            let i = self.idx;
            self.idx += 1;
            let slot = unsafe { self.map.slots.get_unchecked(i) };
            if slot.full {
                return Some((slot.key, slot.val));
            }
        }
        None
    }
}

impl<'a> MapIter<'a> {
    pub(crate) fn next(&mut self) -> Option<(MapKey, Value)> {
        match self {
            MapIter::Inline {
                entries,
                index,
                len,
            } => {
                if *index >= *len {
                    return None;
                }
                let entry = unsafe { entries[*index].assume_init_ref() };
                *index += 1;
                Some(*entry)
            }
            MapIter::HeapGeneric(iter) => iter.next().map(|(k, v)| (*k, *v)),
            MapIter::HeapInt(iter) => iter.next().map(|(k, v)| (MapKey(Value::from_int(k)), v)),
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
    // The inline-cache is aimed at stable maps. For heap maps that are frequently mutated,
    // it becomes pure overhead (version changes every set). Skip it and do a direct lookup.
    unsafe {
        match &(*map).entries {
            MapEntries::HeapGeneric(entries) => {
                if let Some(val) = entries.get(&MapKey(key)).copied() {
                    rc_inc_if_managed(val);
                    return val;
                }
                return Value::nil();
            }
            MapEntries::HeapInt(entries) => {
                let Some(i) = int_value(key) else {
                    return Value::nil();
                };
                if let Some(val) = entries.get(i) {
                    rc_inc_if_managed(val);
                    return val;
                }
                return Value::nil();
            }
            _ => {}
        }
    }
    if let Some(val) = map_get_ic_hit(map, key) {
        #[cfg(test)]
        MAP_IC_HITS.fetch_add(1, Ordering::Relaxed);
        return val;
    }
    #[cfg(test)]
    MAP_IC_MISSES.fetch_add(1, Ordering::Relaxed);
    map_get_ic_miss(map, key)
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
                    rc_inc_if_managed(val);
                    rc_dec_if_managed(old);
                    (*map).version = (*map).version.wrapping_add(1);
                    return;
                }
            }
            if *len < INLINE_CAP {
                entries[*len].write((MapKey(key_val), val));
                *len += 1;
                rc_inc_if_managed(key_val);
                rc_inc_if_managed(val);
                (*map).version = (*map).version.wrapping_add(1);
                return;
            }
        }
        if let MapEntries::Inline { .. } = &mut *entries_ptr {
            // Promote inline -> heap. If all keys are immediate integers, use the int-key fast
            // path to avoid MapKey hashing / value_eq overhead in hot lanes.
            let old = std::mem::replace(&mut *entries_ptr, MapEntries::new_inline());
            if let MapEntries::Inline { len, entries } = old {
                let mut all_int = true;
                let mut int_map = IntMap::new(len + 1);
                let mut generic_map: Option<FastHashMap<MapKey, Value>> = None;
                for idx in 0..len {
                    let (k, v) = entries[idx].assume_init_read();
                    if all_int {
                        if let Some(i) = int_value(k.0) {
                            let _ = int_map.insert(i, v);
                            continue;
                        }
                        all_int = false;
                        let mut g: FastHashMap<MapKey, Value> =
                            HashMap::with_capacity_and_hasher(len + 1, Default::default());
                        // Move existing int entries into the generic map.
                        for (ik, iv) in int_map.iter() {
                            g.insert(MapKey(Value::from_int(ik)), iv);
                        }
                        g.insert(k, v);
                        generic_map = Some(g);
                        continue;
                    }
                    if let Some(g) = generic_map.as_mut() {
                        g.insert(k, v);
                    }
                }
                if all_int {
                    *entries_ptr = MapEntries::HeapInt(int_map);
                } else {
                    *entries_ptr = MapEntries::HeapGeneric(generic_map.unwrap());
                }
            }
        }
        if let MapEntries::HeapInt(entries) = &mut *entries_ptr {
            if let Some(i) = int_value(key_val) {
                if let Some(old) = entries.insert(i, val) {
                    rc_inc_if_managed(val);
                    rc_dec_if_managed(old);
                } else {
                    rc_inc_if_managed(val);
                }
                (*map).version = (*map).version.wrapping_add(1);
                return;
            }
            // Upgrade int-only heap map -> generic for non-int keys.
            let old = std::mem::replace(&mut *entries_ptr, MapEntries::new_inline());
            if let MapEntries::HeapInt(old_int) = old {
                let mut g: FastHashMap<MapKey, Value> =
                    HashMap::with_capacity_and_hasher(old_int.len() + 1, Default::default());
                for (ik, iv) in old_int.iter() {
                    g.insert(MapKey(Value::from_int(ik)), iv);
                }
                *entries_ptr = MapEntries::HeapGeneric(g);
            } else {
                // Put it back. Should be impossible, but don't corrupt the map.
                *entries_ptr = old;
            }
        }
        if let MapEntries::HeapGeneric(entries) = &mut *entries_ptr {
            match entries.entry(MapKey(key_val)) {
                Entry::Occupied(mut entry) => {
                    let old = entry.insert(val);
                    rc_inc_if_managed(val);
                    rc_dec_if_managed(old);
                }
                Entry::Vacant(entry) => {
                    entry.insert(val);
                    rc_inc_if_managed(key_val);
                    rc_inc_if_managed(val);
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
                    rc_dec_if_managed(key.0);
                    rc_dec_if_managed(*val);
                }
            }
            MapEntries::HeapGeneric(entries) => {
                for (key, val) in entries.iter() {
                    rc_dec_if_managed(key.0);
                    rc_dec_if_managed(*val);
                }
            }
            MapEntries::HeapInt(entries) => {
                for (_k, v) in entries.iter() {
                    rc_dec_if_managed(v);
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
                    rc_dec_if_managed(key.0);
                    rc_dec_if_managed(*val);
                }
            }
            MapEntries::HeapGeneric(entries) => {
                for (key, val) in entries.iter() {
                    rc_dec_if_managed(key.0);
                    rc_dec_if_managed(*val);
                }
            }
            MapEntries::HeapInt(entries) => {
                for (_k, v) in entries.iter() {
                    rc_dec_if_managed(v);
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
            MapEntries::HeapGeneric(entries) => entries.len(),
            MapEntries::HeapInt(entries) => entries.len(),
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
            MapEntries::HeapGeneric(entries) => {
                let iter = entries.iter();
                MapIter::HeapGeneric(std::mem::transmute::<
                    HashIter<'_, MapKey, Value>,
                    HashIter<'static, MapKey, Value>,
                >(iter))
            }
            MapEntries::HeapInt(entries) => {
                let iter = entries.iter();
                MapIter::HeapInt(std::mem::transmute::<IntMapIter<'_>, IntMapIter<'static>>(
                    iter,
                ))
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
            MapEntries::HeapGeneric(entries) => entries.get(&key).copied(),
            MapEntries::HeapInt(entries) => {
                let Some(i) = int_value(key.0) else {
                    return None;
                };
                entries.get(i)
            }
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

#[cfg(test)]
pub(crate) fn map_ic_stats() -> (u64, u64) {
    (
        MAP_IC_HITS.load(Ordering::Relaxed),
        MAP_IC_MISSES.load(Ordering::Relaxed),
    )
}

#[cfg(test)]
pub(crate) fn map_ic_reset_stats() {
    MAP_IC_HITS.store(0, Ordering::Relaxed);
    MAP_IC_MISSES.store(0, Ordering::Relaxed);
    MAP_INLINE_CACHE.with(|cache| cache.borrow_mut().clear());
}

fn map_get_ic_hit(map: *mut MapObj, key: Value) -> Option<Value> {
    let version = map_version(map);
    MAP_INLINE_CACHE.with(|cache_cell| {
        let cache = *cache_cell.borrow();
        if !cache.matches(map, version, key) {
            return None;
        }
        unsafe {
            match cache.shape {
                CACHE_SHAPE_INLINE_SLOT => match &(*map).entries {
                    MapEntries::Inline { len, entries } if cache.inline_index < *len => {
                        let (stored_key, val) = entries[cache.inline_index].assume_init_ref();
                        if *stored_key == MapKey(key) {
                            rc_inc_if_managed(*val);
                            Some(*val)
                        } else {
                            None
                        }
                    }
                    _ => None,
                },
                CACHE_SHAPE_HEAP_KEY => match &(*map).entries {
                    MapEntries::HeapGeneric(entries) => {
                        entries.get(&MapKey(key)).copied().inspect(|v| {
                            rc_inc_if_managed(*v);
                        })
                    }
                    _ => None,
                },
                CACHE_SHAPE_MISSING => Some(Value::nil()),
                _ => None,
            }
        }
    })
}

fn map_get_ic_miss(map: *mut MapObj, key: Value) -> Value {
    let version = map_version(map);
    unsafe {
        match &(*map).entries {
            MapEntries::Inline { len, entries } => {
                for idx in 0..*len {
                    let (stored_key, val) = entries[idx].assume_init_ref();
                    if *stored_key == MapKey(key) {
                        MAP_INLINE_CACHE.with(|cache| {
                            cache.borrow_mut().store_inline_slot(map, version, key, idx)
                        });
                        rc_inc_if_managed(*val);
                        return *val;
                    }
                }
            }
            MapEntries::HeapGeneric(entries) => {
                if let Some(val) = entries.get(&MapKey(key)).copied() {
                    MAP_INLINE_CACHE
                        .with(|cache| cache.borrow_mut().store_heap_key(map, version, key));
                    rc_inc_if_managed(val);
                    return val;
                }
            }
            MapEntries::HeapInt(_) => {}
        }
    }
    MAP_INLINE_CACHE.with(|cache| cache.borrow_mut().store_missing(map, version, key));
    Value::nil()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_map_handles_growth_and_lookup() {
        let mut map = IntMap::new(2);
        for idx in 0..256i64 {
            let previous = map.insert(idx, Value::from_int(idx * 3));
            assert!(previous.is_none());
        }
        for idx in 0..256i64 {
            assert!(map.get(idx) == Some(Value::from_int(idx * 3)));
        }
    }

    #[test]
    fn map_set_promotes_inline_storage_to_heap_int_map() {
        let map_value = map_new();
        let map_ptr = map_value.as_ptr() as *mut MapObj;
        for idx in 0..12i64 {
            map_set(map_value, Value::from_int(idx), Value::from_int(idx + 1));
        }
        assert!(!map_is_inline(map_ptr));
        for idx in 0..12i64 {
            let got = map_get(map_value, Value::from_int(idx));
            assert_eq!(int_value(got), Some(idx + 1));
        }
        unsafe { wr_rc_dec(map_value) };
    }

    #[test]
    fn inline_cache_tracks_hit_after_repeated_lookup() {
        map_ic_reset_stats();
        let map_value = map_new();
        map_set(map_value, Value::from_int(7), Value::from_int(77));
        let first = map_get(map_value, Value::from_int(7));
        let second = map_get(map_value, Value::from_int(7));
        assert_eq!(int_value(first), Some(77));
        assert_eq!(int_value(second), Some(77));
        let (hits, misses) = map_ic_stats();
        assert!(misses >= 1, "expected at least one cache miss");
        assert!(hits >= 1, "expected at least one cache hit");
        unsafe { wr_rc_dec(map_value) };
    }
}
