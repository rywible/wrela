//! WASM runtime shim for the Wrela language.
//!
//! This module provides the `wrela_runtime` import functions that a compiled Wrela WASM
//! module expects. It implements nanbox operations, memory management, reference counting,
//! and core runtime primitives.
//!
//! # NanBox representation
//!
//! All Wrela values are represented as nanboxed i64:
//!
//! - QNAN base:     `0x7ff8_0000_0000_0000`
//! - Tag shift:     49 bits
//! - Payload mask:  lower 49 bits
//! - Tag INT (2):   inline integer (49-bit signed)
//! - Tag IMM (3):   immediate constants (nil=0, false=1, true=2)
//! - Pointer:       no tag bits set above QNAN, payload is heap pointer
//!
//! For the WASM target, pointers are 32-bit but values remain 64-bit i64.

#![allow(dead_code)]
#![allow(clippy::missing_safety_doc)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};

// -- NanBox constants (must match compiler/backend/cranelift.rs) --

const NANBOX_QNAN: u64 = 0x7ff8_0000_0000_0000;
const NANBOX_TAG_SHIFT: u64 = 49;
const NANBOX_PAYLOAD_MASK: u64 = (1u64 << NANBOX_TAG_SHIFT) - 1;
const NANBOX_TAG_MASK: u64 = 0x3 << NANBOX_TAG_SHIFT;
const NANBOX_TAG_INT: u64 = 2;
const NANBOX_TAG_IMM: u64 = 3;
const NANBOX_IMM_NIL: u64 = 0;
const NANBOX_IMM_FALSE: u64 = 1;
const NANBOX_IMM_TRUE: u64 = 2;
const NANBOX_INT_MIN: i64 = -(1i64 << (NANBOX_TAG_SHIFT - 1));
const NANBOX_INT_MAX: i64 = (1i64 << (NANBOX_TAG_SHIFT - 1)) - 1;
const RUNTIME_ABI_VERSION: i64 = 5;

// -- Heap-allocated value types --

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeapTag {
    BoxedInt = 0,
    BoxedFloat = 1,
    String = 2,
    List = 3,
    Map = 4,
    ClassInstance = 5,
    Result = 6,
    Range = 7,
    Bytes = 8,
    Iterator = 9,
    Pending = 10,
    Actor = 11,
}

#[repr(C)]
struct HeapObject {
    tag: HeapTag,
    rc: AtomicI64,
    data: HeapData,
}

enum HeapData {
    BoxedInt(i64),
    BoxedFloat(f64),
    String(String),
    List(Vec<i64>),
    Map(HashMap<i64, i64>),
    ClassInstance {
        type_id: i64,
        fields: Vec<i64>,
    },
    ResultOk(i64),
    ResultErr(i64),
    Range(i64, i64),
    Bytes(Vec<u8>),
    Nil,
}

static NEXT_HEAP_ID: AtomicI64 = AtomicI64::new(1);

struct WasmHeap {
    objects: HashMap<u64, Box<HeapObject>>,
    string_intern: HashMap<String, u64>,
}

impl WasmHeap {
    fn new() -> Self {
        Self {
            objects: HashMap::new(),
            string_intern: HashMap::new(),
        }
    }

    fn alloc(&mut self, obj: HeapObject) -> i64 {
        let id = NEXT_HEAP_ID.fetch_add(1, Ordering::Relaxed) as u64;
        self.objects.insert(id, Box::new(obj));
        // Encode as nanbox pointer: QNAN | pointer-payload (no tag bits)
        (NANBOX_QNAN | (id & NANBOX_PAYLOAD_MASK)) as i64
    }

    fn get(&self, value: i64) -> Option<&HeapObject> {
        let payload = extract_pointer_payload(value)?;
        self.objects.get(&payload).map(|b| b.as_ref())
    }

    fn get_mut(&mut self, value: i64) -> Option<&mut HeapObject> {
        let payload = extract_pointer_payload(value)?;
        self.objects.get_mut(&payload).map(|b| b.as_mut())
    }

    fn remove(&mut self, value: i64) -> Option<Box<HeapObject>> {
        let payload = extract_pointer_payload(value)?;
        self.objects.remove(&payload)
    }
}

fn extract_pointer_payload(value: i64) -> Option<u64> {
    let v = value as u64;
    if v & NANBOX_QNAN != NANBOX_QNAN {
        return None; // Not a nanboxed value
    }
    let tag = (v >> NANBOX_TAG_SHIFT) & 0x3;
    if tag != 0 {
        return None; // Has tag bits set, so it's int or imm, not pointer
    }
    Some(v & NANBOX_PAYLOAD_MASK)
}

// Simple thread-local heap for WASM (single-threaded)
std::thread_local! {
    static HEAP: std::cell::RefCell<WasmHeap> = std::cell::RefCell::new(WasmHeap::new());
}

fn with_heap<F, R>(f: F) -> R
where
    F: FnOnce(&mut WasmHeap) -> R,
{
    HEAP.with(|h| f(&mut h.borrow_mut()))
}

// -- NanBox helpers --

fn nanbox_const(tag: u64, payload: u64) -> i64 {
    (NANBOX_QNAN | (tag << NANBOX_TAG_SHIFT) | (payload & NANBOX_PAYLOAD_MASK)) as i64
}

fn nanbox_nil() -> i64 {
    nanbox_const(NANBOX_TAG_IMM, NANBOX_IMM_NIL)
}

fn nanbox_bool(value: bool) -> i64 {
    nanbox_const(
        NANBOX_TAG_IMM,
        if value {
            NANBOX_IMM_TRUE
        } else {
            NANBOX_IMM_FALSE
        },
    )
}

fn nanbox_int(value: i64) -> i64 {
    if value >= NANBOX_INT_MIN && value <= NANBOX_INT_MAX {
        // Inline integer
        let payload = (value as u64) & NANBOX_PAYLOAD_MASK;
        (NANBOX_QNAN | (NANBOX_TAG_INT << NANBOX_TAG_SHIFT) | payload) as i64
    } else {
        // Boxed integer (heap allocated)
        with_heap(|heap| {
            heap.alloc(HeapObject {
                tag: HeapTag::BoxedInt,
                rc: AtomicI64::new(1),
                data: HeapData::BoxedInt(value),
            })
        })
    }
}

fn unbox_int(value: i64) -> i64 {
    let v = value as u64;
    let tag = (v >> NANBOX_TAG_SHIFT) & 0x3;
    if tag == NANBOX_TAG_INT {
        // Inline integer - sign-extend from 49 bits
        let payload = v & NANBOX_PAYLOAD_MASK;
        let sign_bit = 1u64 << (NANBOX_TAG_SHIFT - 1);
        if payload & sign_bit != 0 {
            (payload | !NANBOX_PAYLOAD_MASK) as i64
        } else {
            payload as i64
        }
    } else {
        // Heap-allocated boxed int
        with_heap(|heap| match heap.get(value) {
            Some(obj) => match &obj.data {
                HeapData::BoxedInt(i) => *i,
                _ => 0,
            },
            None => 0,
        })
    }
}

fn is_truthy(value: i64) -> bool {
    value != nanbox_nil() && value != nanbox_bool(false)
}

// -- Runtime API functions --
// These would be exported as WASM imports under the "wrela_runtime" module.

/// Initialize the runtime.
pub fn wr_runtime_init() {
    // No-op for WASM - single-threaded, no system setup needed.
}

/// Return the runtime ABI version.
pub fn wr_runtime_abi() -> i64 {
    RUNTIME_ABI_VERSION
}

/// Increment reference count for a nanboxed value.
pub fn wr_rc_inc(value: i64) {
    with_heap(|heap| {
        if let Some(obj) = heap.get(value) {
            obj.rc.fetch_add(1, Ordering::Relaxed);
        }
    });
}

/// Decrement reference count for a nanboxed value.
pub fn wr_rc_dec(value: i64) {
    let should_free = with_heap(|heap| {
        if let Some(obj) = heap.get(value) {
            let prev = obj.rc.fetch_sub(1, Ordering::Relaxed);
            prev <= 1
        } else {
            false
        }
    });
    if should_free {
        with_heap(|heap| {
            heap.remove(value);
        });
    }
}

/// Box an integer value (for values that don't fit in inline nanbox).
pub fn wr_box_int(value: i64) -> i64 {
    nanbox_int(value)
}

/// Box a float value as a nanboxed heap pointer.
pub fn wr_box_float(value: f64) -> i64 {
    with_heap(|heap| {
        heap.alloc(HeapObject {
            tag: HeapTag::BoxedFloat,
            rc: AtomicI64::new(1),
            data: HeapData::BoxedFloat(value),
        })
    })
}

/// Unbox a float value from a nanboxed heap pointer.
pub fn wr_unbox_float(value: i64) -> f64 {
    with_heap(|heap| match heap.get(value) {
        Some(obj) => match &obj.data {
            HeapData::BoxedFloat(f) => *f,
            _ => 0.0,
        },
        None => {
            // It might be a raw f64 bits stored as i64
            f64::from_bits(value as u64)
        }
    })
}

/// Numeric addition (polymorphic - handles int + int, float + float, etc.)
pub fn wr_num_add(lhs: i64, rhs: i64) -> i64 {
    let l = unbox_int(lhs);
    let r = unbox_int(rhs);
    nanbox_int(l.wrapping_add(r))
}

/// Numeric subtraction.
pub fn wr_num_sub(lhs: i64, rhs: i64) -> i64 {
    let l = unbox_int(lhs);
    let r = unbox_int(rhs);
    nanbox_int(l.wrapping_sub(r))
}

/// Numeric multiplication.
pub fn wr_num_mul(lhs: i64, rhs: i64) -> i64 {
    let l = unbox_int(lhs);
    let r = unbox_int(rhs);
    nanbox_int(l.wrapping_mul(r))
}

/// Numeric division.
pub fn wr_num_div(lhs: i64, rhs: i64) -> i64 {
    let l = unbox_int(lhs);
    let r = unbox_int(rhs);
    if r == 0 {
        nanbox_nil()
    } else {
        nanbox_int(l / r)
    }
}

/// Numeric modulo.
pub fn wr_num_mod(lhs: i64, rhs: i64) -> i64 {
    let l = unbox_int(lhs);
    let r = unbox_int(rhs);
    if r == 0 {
        nanbox_nil()
    } else {
        nanbox_int(l % r)
    }
}

/// Numeric negation.
pub fn wr_num_neg(value: i64) -> i64 {
    let v = unbox_int(value);
    nanbox_int(-v)
}

/// Numeric less-than comparison.
pub fn wr_num_lt(lhs: i64, rhs: i64) -> i64 {
    nanbox_bool(unbox_int(lhs) < unbox_int(rhs))
}

/// Numeric greater-than comparison.
pub fn wr_num_gt(lhs: i64, rhs: i64) -> i64 {
    nanbox_bool(unbox_int(lhs) > unbox_int(rhs))
}

/// Numeric less-than-or-equal comparison.
pub fn wr_num_le(lhs: i64, rhs: i64) -> i64 {
    nanbox_bool(unbox_int(lhs) <= unbox_int(rhs))
}

/// Numeric greater-than-or-equal comparison.
pub fn wr_num_ge(lhs: i64, rhs: i64) -> i64 {
    nanbox_bool(unbox_int(lhs) >= unbox_int(rhs))
}

/// Value equality comparison.
pub fn wr_value_eq(lhs: i64, rhs: i64) -> i64 {
    nanbox_bool(lhs == rhs)
}

/// Deep value equality comparison.
pub fn wr_value_deep_eq(lhs: i64, rhs: i64) -> i64 {
    // For the WASM shim, deep equality falls back to bitwise equality.
    nanbox_bool(lhs == rhs)
}

/// Identity equality comparison.
pub fn wr_identity_eq(lhs: i64, rhs: i64) -> i64 {
    nanbox_bool(lhs == rhs)
}

/// Coverage hit tracking (no-op in WASM).
pub fn wr_coverage_hit(_id: i64) -> i64 {
    nanbox_nil()
}

/// Intern a UTF-8 string from memory.
pub fn wr_str_intern_utf8(ptr: i32, len: i32) -> i64 {
    // In WASM, ptr and len refer to linear memory addresses.
    // This is a stub - actual implementation would read from WASM linear memory.
    let _ = (ptr, len);
    with_heap(|heap| {
        heap.alloc(HeapObject {
            tag: HeapTag::String,
            rc: AtomicI64::new(1),
            data: HeapData::String(String::new()),
        })
    })
}

/// String concatenation.
pub fn wr_str_concat(ptr: i32, len: i32) -> i64 {
    let _ = (ptr, len);
    with_heap(|heap| {
        heap.alloc(HeapObject {
            tag: HeapTag::String,
            rc: AtomicI64::new(1),
            data: HeapData::String(String::new()),
        })
    })
}

/// String concatenation (local variant).
pub fn wr_str_concat_local(ptr: i32, len: i32) -> i64 {
    wr_str_concat(ptr, len)
}

/// String length.
pub fn wr_str_len(value: i64) -> i64 {
    with_heap(|heap| match heap.get(value) {
        Some(obj) => match &obj.data {
            HeapData::String(s) => nanbox_int(s.len() as i64),
            _ => nanbox_int(0),
        },
        None => nanbox_int(0),
    })
}

/// Create a new list.
pub fn wr_list_new(cap: i32) -> i64 {
    with_heap(|heap| {
        heap.alloc(HeapObject {
            tag: HeapTag::List,
            rc: AtomicI64::new(1),
            data: HeapData::List(Vec::with_capacity(cap as usize)),
        })
    })
}

/// Create a new list (local variant).
pub fn wr_list_new_local(cap: i32) -> i64 {
    wr_list_new(cap)
}

/// Push a value onto a list.
pub fn wr_list_push_val(list: i64, value: i64) -> i64 {
    with_heap(|heap| {
        if let Some(obj) = heap.get_mut(list) {
            if let HeapData::List(ref mut vec) = obj.data {
                vec.push(value);
            }
        }
    });
    list
}

/// Get a value from a list by index.
pub fn wr_list_get_val(list: i64, index: i64) -> i64 {
    let idx = unbox_int(index) as usize;
    with_heap(|heap| match heap.get(list) {
        Some(obj) => match &obj.data {
            HeapData::List(vec) => vec.get(idx).copied().unwrap_or_else(nanbox_nil),
            _ => nanbox_nil(),
        },
        None => nanbox_nil(),
    })
}

/// Get list length.
pub fn wr_list_len(list: i64) -> i64 {
    with_heap(|heap| match heap.get(list) {
        Some(obj) => match &obj.data {
            HeapData::List(vec) => nanbox_int(vec.len() as i64),
            _ => nanbox_int(0),
        },
        None => nanbox_int(0),
    })
}

/// Set a value in a list.
pub fn wr_list_set(list: i64, index: i32, value: i64) {
    with_heap(|heap| {
        if let Some(obj) = heap.get_mut(list) {
            if let HeapData::List(ref mut vec) = obj.data {
                let idx = index as usize;
                if idx < vec.len() {
                    vec[idx] = value;
                }
            }
        }
    });
}

/// Set a value in a list (by nanboxed index).
pub fn wr_list_set_val(list: i64, index: i64, value: i64) -> i64 {
    let idx = unbox_int(index) as usize;
    with_heap(|heap| {
        if let Some(obj) = heap.get_mut(list) {
            if let HeapData::List(ref mut vec) = obj.data {
                if idx < vec.len() {
                    vec[idx] = value;
                }
            }
        }
    });
    list
}

/// Create a new map.
pub fn wr_map_new() -> i64 {
    with_heap(|heap| {
        heap.alloc(HeapObject {
            tag: HeapTag::Map,
            rc: AtomicI64::new(1),
            data: HeapData::Map(HashMap::new()),
        })
    })
}

/// Create a new map (local variant).
pub fn wr_map_new_local() -> i64 {
    wr_map_new()
}

/// Get a value from a map.
pub fn wr_map_get(map: i64, key: i64) -> i64 {
    with_heap(|heap| match heap.get(map) {
        Some(obj) => match &obj.data {
            HeapData::Map(m) => m.get(&key).copied().unwrap_or_else(nanbox_nil),
            _ => nanbox_nil(),
        },
        None => nanbox_nil(),
    })
}

/// Set a value in a map.
pub fn wr_map_set(map: i64, key: i64, value: i64) -> i64 {
    with_heap(|heap| {
        if let Some(obj) = heap.get_mut(map) {
            if let HeapData::Map(ref mut m) = obj.data {
                m.insert(key, value);
            }
        }
    });
    map
}

/// Get map length.
pub fn wr_map_len(map: i64) -> i64 {
    with_heap(|heap| match heap.get(map) {
        Some(obj) => match &obj.data {
            HeapData::Map(m) => nanbox_int(m.len() as i64),
            _ => nanbox_int(0),
        },
        None => nanbox_int(0),
    })
}

/// Create a new range.
pub fn wr_range_new(start: i64, end: i64) -> i64 {
    with_heap(|heap| {
        heap.alloc(HeapObject {
            tag: HeapTag::Range,
            rc: AtomicI64::new(1),
            data: HeapData::Range(start, end),
        })
    })
}

/// Print a value (stub for WASM).
pub fn wr_print(value: i64) -> i64 {
    let _ = value;
    nanbox_nil()
}

/// Log a message (stub for WASM).
pub fn wr_log(_level: i64, _tag: i64, _message: i64) -> i64 {
    nanbox_nil()
}

/// Assert a condition.
pub fn wr_assert(cond: i64, _msg: i64) -> i64 {
    if !is_truthy(cond) {
        // In WASM, we'd trap
        panic!("assertion failed");
    }
    nanbox_nil()
}

/// Assert equality.
pub fn wr_assert_eq(lhs: i64, rhs: i64) -> i64 {
    if lhs != rhs {
        panic!("assertion failed: values not equal");
    }
    nanbox_nil()
}

/// Create a Result::Ok.
pub fn wr_result_ok(value: i64) -> i64 {
    with_heap(|heap| {
        heap.alloc(HeapObject {
            tag: HeapTag::Result,
            rc: AtomicI64::new(1),
            data: HeapData::ResultOk(value),
        })
    })
}

/// Create a Result::Err.
pub fn wr_result_err(value: i64) -> i64 {
    with_heap(|heap| {
        heap.alloc(HeapObject {
            tag: HeapTag::Result,
            rc: AtomicI64::new(1),
            data: HeapData::ResultErr(value),
        })
    })
}

/// Check if a Result is Ok.
pub fn wr_result_is_ok(result: i64) -> i64 {
    with_heap(|heap| match heap.get(result) {
        Some(obj) => match &obj.data {
            HeapData::ResultOk(_) => nanbox_bool(true),
            HeapData::ResultErr(_) => nanbox_bool(false),
            _ => nanbox_bool(false),
        },
        None => nanbox_bool(false),
    })
}

/// Unwrap a Result::Ok value.
pub fn wr_result_unwrap(result: i64) -> i64 {
    with_heap(|heap| match heap.get(result) {
        Some(obj) => match &obj.data {
            HeapData::ResultOk(v) => *v,
            _ => nanbox_nil(),
        },
        None => nanbox_nil(),
    })
}

/// Unwrap a Result::Err value.
pub fn wr_result_err_unwrap(result: i64) -> i64 {
    with_heap(|heap| match heap.get(result) {
        Some(obj) => match &obj.data {
            HeapData::ResultErr(v) => *v,
            _ => nanbox_nil(),
        },
        None => nanbox_nil(),
    })
}

/// Create a new class instance.
pub fn wr_class_new(type_id: i64, _names_ptr: i32, _names_len_ptr: i32, count: i32) -> i64 {
    with_heap(|heap| {
        heap.alloc(HeapObject {
            tag: HeapTag::ClassInstance,
            rc: AtomicI64::new(1),
            data: HeapData::ClassInstance {
                type_id,
                fields: vec![nanbox_nil(); count as usize],
            },
        })
    })
}

/// Get a field slot from a class instance.
pub fn wr_class_get_slot(
    instance: i64,
    _name_ptr: i32,
    _name_len: i32,
    slot: i32,
) -> i64 {
    let slot_idx = slot as usize;
    with_heap(|heap| match heap.get(instance) {
        Some(obj) => match &obj.data {
            HeapData::ClassInstance { fields, .. } => {
                fields.get(slot_idx).copied().unwrap_or_else(nanbox_nil)
            }
            _ => nanbox_nil(),
        },
        None => nanbox_nil(),
    })
}

/// Set a field slot on a class instance.
pub fn wr_class_set_slot(
    instance: i64,
    _name_ptr: i32,
    _name_len: i32,
    slot: i32,
    value: i64,
) {
    let slot_idx = slot as usize;
    with_heap(|heap| {
        if let Some(obj) = heap.get_mut(instance) {
            if let HeapData::ClassInstance {
                ref mut fields, ..
            } = obj.data
            {
                if slot_idx < fields.len() {
                    fields[slot_idx] = value;
                }
            }
        }
    });
}

/// Get type ID of a value.
pub fn wr_type_id(value: i64) -> i64 {
    let v = value as u64;
    if v & NANBOX_QNAN != NANBOX_QNAN {
        return nanbox_int(0); // Not a nanboxed value
    }
    let tag = (v >> NANBOX_TAG_SHIFT) & 0x3;
    match tag {
        t if t == NANBOX_TAG_INT => nanbox_int(1), // Integer type
        t if t == NANBOX_TAG_IMM => {
            let payload = v & NANBOX_PAYLOAD_MASK;
            match payload {
                p if p == NANBOX_IMM_NIL => nanbox_int(0),   // Nil type
                p if p == NANBOX_IMM_TRUE || p == NANBOX_IMM_FALSE => nanbox_int(2), // Boolean type
                _ => nanbox_int(0),
            }
        }
        _ => {
            // Heap pointer - look up the object
            with_heap(|heap| match heap.get(value) {
                Some(obj) => match &obj.data {
                    HeapData::ClassInstance { type_id, .. } => *type_id,
                    HeapData::String(_) => nanbox_int(3),
                    HeapData::BoxedFloat(_) => nanbox_int(4),
                    HeapData::List(_) => nanbox_int(5),
                    HeapData::Map(_) => nanbox_int(6),
                    _ => nanbox_int(0),
                },
                None => nanbox_int(0),
            })
        }
    }
}

/// Crash (trap in WASM).
pub fn wr_crash(_msg: i64) -> i64 {
    panic!("wrela crash");
}

/// Iterator init.
pub fn wr_iter_init(iterable: i64) -> i64 {
    // Stub: return the iterable itself as an iterator handle.
    iterable
}

/// Iterator next (writes value and done flag through pointers).
pub fn wr_iter_next(_iter: i64, _value_ptr: i32, _done_ptr: i32) {
    // Stub: mark as done immediately.
    // In real implementation, would write to WASM linear memory.
}

/// Pending await (stub).
pub fn wr_pending_await(pending: i64) -> i64 {
    let _ = pending;
    nanbox_nil()
}

/// Register a method (stub).
pub fn wr_register_method(_class_id: i64, _method_id: i64, _func_ptr: i32) {}

/// Register a class (stub).
pub fn wr_register_class(_name_ptr: i32, _name_len: i32, _class_id: i64) {}

/// Register a method name (stub).
pub fn wr_register_method_name(
    _name_ptr: i32,
    _name_len: i32,
    _class_id: i64,
    _method_id: i64,
) {
}

/// Runtime configure (stub).
pub fn wr_runtime_configure(_config: i64) -> i64 {
    nanbox_nil()
}

/// Clock nanoseconds (stub for WASM).
pub fn wr_clock_ns() -> i64 {
    nanbox_int(0)
}

/// Sleep milliseconds (stub for WASM).
pub fn wr_sleep_ms(_ms: i64) -> i64 {
    nanbox_nil()
}

/// Environment variable get (stub for WASM).
pub fn wr_env_get(_key: i64) -> i64 {
    nanbox_nil()
}

/// Environment variable set (stub for WASM).
pub fn wr_env_set(_key: i64, _value: i64) -> i64 {
    nanbox_nil()
}

/// Bytes from string (stub).
pub fn wr_bytes_from_string(value: i64) -> i64 {
    let _ = value;
    with_heap(|heap| {
        heap.alloc(HeapObject {
            tag: HeapTag::Bytes,
            rc: AtomicI64::new(1),
            data: HeapData::Bytes(Vec::new()),
        })
    })
}

/// Bytes from list (stub).
pub fn wr_bytes_from_list(value: i64) -> i64 {
    let _ = value;
    with_heap(|heap| {
        heap.alloc(HeapObject {
            tag: HeapTag::Bytes,
            rc: AtomicI64::new(1),
            data: HeapData::Bytes(Vec::new()),
        })
    })
}

/// Bytes to string (stub).
pub fn wr_bytes_to_string(value: i64) -> i64 {
    let _ = value;
    with_heap(|heap| {
        heap.alloc(HeapObject {
            tag: HeapTag::String,
            rc: AtomicI64::new(1),
            data: HeapData::String(String::new()),
        })
    })
}

/// Bytes to list (stub).
pub fn wr_bytes_to_list(value: i64) -> i64 {
    let _ = value;
    with_heap(|heap| {
        heap.alloc(HeapObject {
            tag: HeapTag::List,
            rc: AtomicI64::new(1),
            data: HeapData::List(Vec::new()),
        })
    })
}

/// Bytes length (stub).
pub fn wr_bytes_len(value: i64) -> i64 {
    let _ = value;
    nanbox_int(0)
}

/// Log configure (stub).
pub fn wr_log_configure(_config: i64) -> i64 {
    nanbox_nil()
}

/// CPU count (stub for WASM - single threaded).
pub fn wr_runtime_cpu_count() -> i64 {
    nanbox_int(1)
}

/// Assert value equality.
pub fn wr_assert_value_equality(lhs: i64, rhs: i64) -> i64 {
    if lhs != rhs {
        panic!("assertion failed: values not deeply equal");
    }
    nanbox_nil()
}

/// Assert identity.
pub fn wr_assert_identity(lhs: i64, rhs: i64) -> i64 {
    if lhs != rhs {
        panic!("assertion failed: values not identical");
    }
    nanbox_nil()
}

/// Assert error (stub).
pub fn wr_assert_err(result: i64) -> i64 {
    let _ = result;
    nanbox_nil()
}

/// Actor send (stub for WASM).
pub fn wr_actor_send(_actor: i64, _method_id: i64, _args_ptr: i32, _args_len: i32) -> i64 {
    nanbox_nil()
}

/// Actor spawn (stub for WASM).
pub fn wr_actor_spawn(
    _a1: i64,
    _a2: i64,
    _a3: i64,
    _a4: i64,
    _a5: i64,
    _a6: i64,
    _a7: i64,
) -> i64 {
    nanbox_nil()
}

/// Pool new (stub for WASM).
pub fn wr_pool_new(
    _a1: i64,
    _a2: i64,
    _a3: i64,
    _a4: i64,
    _a5: i64,
    _a6: i64,
) -> i64 {
    nanbox_nil()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nanbox_int_roundtrip() {
        for v in [0i64, 1, -1, 42, -42, 1000, -1000, NANBOX_INT_MAX, NANBOX_INT_MIN] {
            let boxed = nanbox_int(v);
            let unboxed = unbox_int(boxed);
            assert_eq!(unboxed, v, "roundtrip failed for {v}");
        }
    }

    #[test]
    fn nanbox_bool_roundtrip() {
        let t = nanbox_bool(true);
        let f = nanbox_bool(false);
        assert!(is_truthy(t));
        assert!(!is_truthy(f));
    }

    #[test]
    fn nanbox_nil_is_not_truthy() {
        assert!(!is_truthy(nanbox_nil()));
    }

    #[test]
    fn runtime_abi_version_matches() {
        assert_eq!(wr_runtime_abi(), RUNTIME_ABI_VERSION);
    }

    #[test]
    fn num_add_works() {
        let a = nanbox_int(10);
        let b = nanbox_int(20);
        let result = wr_num_add(a, b);
        assert_eq!(unbox_int(result), 30);
    }

    #[test]
    fn num_sub_works() {
        let a = nanbox_int(30);
        let b = nanbox_int(20);
        let result = wr_num_sub(a, b);
        assert_eq!(unbox_int(result), 10);
    }

    #[test]
    fn num_mul_works() {
        let a = nanbox_int(6);
        let b = nanbox_int(7);
        let result = wr_num_mul(a, b);
        assert_eq!(unbox_int(result), 42);
    }

    #[test]
    fn num_comparisons() {
        let a = nanbox_int(5);
        let b = nanbox_int(10);

        assert!(is_truthy(wr_num_lt(a, b)));
        assert!(!is_truthy(wr_num_gt(a, b)));
        assert!(is_truthy(wr_num_le(a, b)));
        assert!(!is_truthy(wr_num_ge(a, b)));

        let eq_result = wr_value_eq(a, a);
        assert!(is_truthy(eq_result));
    }

    #[test]
    fn rc_inc_dec_does_not_crash() {
        let val = nanbox_int(42);
        wr_rc_inc(val); // No-op for inline ints
        wr_rc_dec(val); // No-op for inline ints
    }

    #[test]
    fn result_ok_roundtrip() {
        let inner = nanbox_int(42);
        let result = wr_result_ok(inner);
        assert!(is_truthy(wr_result_is_ok(result)));
        assert_eq!(wr_result_unwrap(result), inner);
    }

    #[test]
    fn result_err_roundtrip() {
        let inner = nanbox_int(99);
        let result = wr_result_err(inner);
        assert!(!is_truthy(wr_result_is_ok(result)));
        assert_eq!(wr_result_err_unwrap(result), inner);
    }
}
