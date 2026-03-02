use crate::object::ObjHeader;
use crate::value::{TypeId, Value, header_raw};
use crate::{wr_rc_dec, wr_rc_inc};
use std::collections::HashMap;

const INVALID_SLOT: u32 = u32::MAX;

#[repr(C)]
pub struct ClassObj {
    header: ObjHeader,
    slot_names: HashMap<Vec<u8>, u32>,
    slots: Box<[Value]>,
    overflow: HashMap<Vec<u8>, Value>,
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
    let mut slot_names = HashMap::new();
    let slots = vec![Value::nil(); count];
    for i in 0..count {
        let name_ptr = unsafe { *names_ptr.add(i) };
        let len = unsafe { *lens_ptr.add(i) };
        if name_ptr.is_null() && len != 0 {
            continue;
        }
        let bytes = unsafe { std::slice::from_raw_parts(name_ptr, len) };
        slot_names.entry(bytes.to_vec()).or_insert(i as u32);
    }
    let obj = Box::new(ClassObj {
        header: header_raw(class_id.max(TypeId::UserBase as u32)),
        slot_names,
        slots: slots.into_boxed_slice(),
        overflow: HashMap::new(),
    });
    Value::from_ptr(Box::into_raw(obj) as *mut ObjHeader)
}

/// If `val` is a user object allocated in the current arena, clone it onto the heap.
///
/// Detached actors must own a stable instance across threads and across arena resets. If we
/// accidentally pass a local-temp arena allocation into `wr_actor_spawn`, we promote it here
/// to preserve semantics.
pub fn promote_user_object_to_heap(val: Value) -> Value {
    let Some(obj) = as_class(val) else {
        return val;
    };
    // Clone unconditionally: actor state must be heap-stable, and this avoids subtle
    // dependencies on whether a temp arena is currently active in the spawning thread.
    crate::metrics::inc_actor_spawn_instance_promoted();
    unsafe {
        let slots_ref = &(*obj).slots;
        let mut slots: Vec<Value> = Vec::with_capacity(slots_ref.len());
        for v in slots_ref.iter().copied() {
            wr_rc_inc(v);
            slots.push(v);
        }
        let mut overflow: HashMap<Vec<u8>, Value> = HashMap::new();
        for (k, v) in (*obj).overflow.iter() {
            wr_rc_inc(*v);
            overflow.insert(k.clone(), *v);
        }
        let new_obj = Box::new(ClassObj {
            header: header_raw((*obj).header.type_id),
            slot_names: (*obj).slot_names.clone(),
            slots: slots.into_boxed_slice(),
            overflow,
        });
        Value::from_ptr(Box::into_raw(new_obj) as *mut ObjHeader)
    }
}

pub fn class_get(obj_val: Value, name_ptr: *const u8, len: usize) -> Value {
    class_get_slot(obj_val, name_ptr, len, INVALID_SLOT)
}

pub fn class_get_slot(obj_val: Value, name_ptr: *const u8, len: usize, slot: u32) -> Value {
    let obj = match as_class(obj_val) {
        Some(obj) => obj,
        None => return Value::nil(),
    };
    if let Some(slot_index) = to_slot_index(slot, unsafe { (&(*obj).slots).len() }) {
        unsafe {
            let val = (&(*obj).slots)[slot_index];
            wr_rc_inc(val);
            return val;
        }
    }
    if name_ptr.is_null() && len != 0 {
        return Value::nil();
    }
    let key = unsafe { std::slice::from_raw_parts(name_ptr, len) };
    unsafe {
        if let Some(slot_index) = (*obj).slot_names.get(key) {
            let val = (&(*obj).slots)[*slot_index as usize];
            wr_rc_inc(val);
            return val;
        }
        if let Some(val) = (*obj).overflow.get(key).copied() {
            wr_rc_inc(val);
            return val;
        }
    }
    Value::nil()
}

pub fn class_set(obj_val: Value, name_ptr: *const u8, len: usize, val: Value) {
    class_set_slot(obj_val, name_ptr, len, INVALID_SLOT, val)
}

pub fn class_set_slot(obj_val: Value, name_ptr: *const u8, len: usize, slot: u32, val: Value) {
    let obj = match as_class(obj_val) {
        Some(obj) => obj,
        None => return,
    };
    if let Some(slot_index) = to_slot_index(slot, unsafe { (&(*obj).slots).len() }) {
        unsafe {
            let old = &mut (*obj).slots[slot_index];
            wr_rc_inc(val);
            wr_rc_dec(*old);
            *old = val;
        }
        return;
    }
    if name_ptr.is_null() && len != 0 {
        return;
    }
    let key = unsafe { std::slice::from_raw_parts(name_ptr, len) };
    unsafe {
        if let Some(slot_index) = (*obj).slot_names.get(key).copied() {
            let old = &mut (*obj).slots[slot_index as usize];
            wr_rc_inc(val);
            wr_rc_dec(*old);
            *old = val;
            return;
        }
        // RC-inc val BEFORE inserting: if val is the same object as the old
        // value and has rc=1, the dec would free it before we finish storing.
        wr_rc_inc(val);
        if let Some(old) = (*obj).overflow.insert(key.to_vec(), val) {
            wr_rc_dec(old);
        }
    }
}

/// # Safety
/// `ptr` must be a valid pointer produced by `class_new`/`Box::into_raw` for `ClassObj`,
/// and this function must be called at most once for that allocation.
pub unsafe fn drop_class(ptr: *mut ObjHeader) {
    let obj = ptr as *mut ClassObj;
    unsafe {
        for val in (*obj).slots.iter() {
            wr_rc_dec(*val);
        }
        for (_key, val) in (*obj).overflow.iter() {
            wr_rc_dec(*val);
        }
        drop(Box::from_raw(obj));
    }
}

pub fn drop_class_in_arena(ptr: *mut ObjHeader) {
    let obj = ptr as *mut ClassObj;
    unsafe {
        for val in (*obj).slots.iter() {
            wr_rc_dec(*val);
        }
        for (_key, val) in (*obj).overflow.iter() {
            wr_rc_dec(*val);
        }
        std::ptr::drop_in_place(&mut (*obj).slot_names);
        std::ptr::drop_in_place(&mut (*obj).slots);
        std::ptr::drop_in_place(&mut (*obj).overflow);
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

pub(crate) fn class_type_and_fields(val: Value) -> Option<(u32, Vec<(Vec<u8>, Value)>)> {
    let obj = as_class(val)?;
    unsafe {
        let mut slots: Vec<(usize, Vec<u8>)> = (*obj)
            .slot_names
            .iter()
            .map(|(name, idx)| (*idx as usize, name.clone()))
            .collect();
        slots.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
        let mut fields = Vec::with_capacity(slots.len().saturating_add((*obj).overflow.len()));
        for (slot_index, name) in slots {
            if let Some(value) = (&(*obj).slots).get(slot_index).copied() {
                fields.push((name, value));
            }
        }
        let mut overflow: Vec<(Vec<u8>, Value)> = (*obj)
            .overflow
            .iter()
            .map(|(name, value)| (name.clone(), *value))
            .collect();
        overflow.sort_by(|left, right| left.0.cmp(&right.0));
        fields.extend(overflow);
        Some(((*obj).header.type_id, fields))
    }
}

fn to_slot_index(slot: u32, len: usize) -> Option<usize> {
    if slot == INVALID_SLOT {
        return None;
    }
    let index = slot as usize;
    if index < len { Some(index) } else { None }
}
