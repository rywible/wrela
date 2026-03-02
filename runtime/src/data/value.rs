use crate::bytes::with_bytes;
use crate::object::ObjHeader;
use crate::string::with_string_bytes;
use std::collections::HashSet;
use std::sync::atomic::AtomicU32;

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Value(pub u64);

impl Default for Value {
    fn default() -> Self {
        Self::nil()
    }
}

impl Value {
    const QNAN: u64 = 0x7ff8_0000_0000_0000;
    const TAG_SHIFT: u64 = 49;
    const TAG_MASK: u64 = 0x3 << Self::TAG_SHIFT;
    const PAYLOAD_MASK: u64 = (1u64 << Self::TAG_SHIFT) - 1;

    const TAG_PTR: u64 = 1;
    const TAG_INT: u64 = 2;
    const TAG_IMM: u64 = 3;

    const IMM_NIL: u64 = 0;
    const IMM_FALSE: u64 = 1;
    const IMM_TRUE: u64 = 2;
    const MIN_INT: i64 = -(1i64 << (Self::TAG_SHIFT - 1));
    const MAX_INT: i64 = (1i64 << (Self::TAG_SHIFT - 1)) - 1;

    #[inline]
    pub const fn nil() -> Self {
        Self::from_nanbox(Self::TAG_IMM, Self::IMM_NIL)
    }

    #[inline]
    pub fn from_int(v: i64) -> Self {
        if !(Self::MIN_INT..=Self::MAX_INT).contains(&v) {
            return box_int(v);
        }
        let payload = (v as u64) & Self::PAYLOAD_MASK;
        Self::from_nanbox(Self::TAG_INT, payload)
    }

    #[inline]
    pub const fn from_bool(v: bool) -> Self {
        if v {
            Self::from_nanbox(Self::TAG_IMM, Self::IMM_TRUE)
        } else {
            Self::from_nanbox(Self::TAG_IMM, Self::IMM_FALSE)
        }
    }

    #[inline]
    pub fn from_float(v: f64) -> Self {
        if v.is_nan() {
            Value(Self::QNAN)
        } else {
            Value(v.to_bits())
        }
    }

    #[inline]
    pub fn from_ptr(ptr: *mut ObjHeader) -> Self {
        debug_assert!(!ptr.is_null());
        let raw = ptr as u64;
        debug_assert!(raw <= Self::PAYLOAD_MASK);
        Self::from_nanbox(Self::TAG_PTR, raw)
    }

    #[inline]
    pub const fn is_ptr(self) -> bool {
        self.is_nanbox() && self.tag() == Self::TAG_PTR
    }

    #[inline]
    pub const fn is_int(self) -> bool {
        self.is_nanbox() && self.tag() == Self::TAG_INT
    }

    #[inline]
    pub const fn is_bool(self) -> bool {
        self.is_nanbox()
            && self.tag() == Self::TAG_IMM
            && (self.payload() == Self::IMM_FALSE || self.payload() == Self::IMM_TRUE)
    }

    #[inline]
    pub const fn is_nil(self) -> bool {
        self.is_nanbox() && self.tag() == Self::TAG_IMM && self.payload() == Self::IMM_NIL
    }

    #[inline]
    pub const fn is_float(self) -> bool {
        !self.is_nanbox()
    }

    #[inline]
    pub const fn as_int(self) -> i64 {
        let payload = self.payload();
        let sign_bit = 1u64 << (Self::TAG_SHIFT - 1);
        let mut val = payload as i64;
        if payload & sign_bit != 0 {
            val |= !((1i64 << Self::TAG_SHIFT) - 1);
        }
        val
    }

    #[inline]
    pub const fn as_bool(self) -> bool {
        self.payload() == Self::IMM_TRUE
    }

    #[inline]
    pub const fn as_ptr(self) -> *mut ObjHeader {
        self.payload() as *mut ObjHeader
    }

    #[inline]
    pub fn as_float(self) -> f64 {
        f64::from_bits(self.0)
    }

    #[inline]
    const fn is_nanbox(self) -> bool {
        (self.0 & Self::QNAN) == Self::QNAN && (self.0 & Self::TAG_MASK) != 0
    }

    #[inline]
    const fn tag(self) -> u64 {
        (self.0 & Self::TAG_MASK) >> Self::TAG_SHIFT
    }

    #[inline]
    const fn payload(self) -> u64 {
        self.0 & Self::PAYLOAD_MASK
    }

    #[inline]
    const fn from_nanbox(tag: u64, payload: u64) -> Self {
        Value(Self::QNAN | (tag << Self::TAG_SHIFT) | (payload & Self::PAYLOAD_MASK))
    }
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TypeId {
    Unknown = 0,
    Integer = 1,
    Boolean = 2,
    Nil = 3,
    Float = 4,
    String = 5,
    List = 6,
    Map = 7,
    Actor = 8,
    Pending = 9,
    Iterator = 10,
    Result = 11,
    Pool = 12,
    Bytes = 13,
    BoxedInteger = 14,
    UserBase = 100,
}

pub fn type_id_raw(val: Value) -> u32 {
    if val.is_int() {
        return TypeId::Integer as u32;
    }
    if val.is_bool() {
        return TypeId::Boolean as u32;
    }
    if val.is_nil() {
        return TypeId::Nil as u32;
    }
    if val.is_float() {
        return TypeId::Float as u32;
    }
    if val.is_ptr() {
        unsafe {
            let header = &*val.as_ptr();
            if header.type_id == TypeId::BoxedInteger as u32 {
                return TypeId::Integer as u32;
            }
            return header.type_id;
        }
    }
    TypeId::Unknown as u32
}

pub fn header(type_id: TypeId) -> ObjHeader {
    ObjHeader {
        rc: AtomicU32::new(1),
        type_id: type_id as u32,
    }
}

pub fn header_raw(type_id: u32) -> ObjHeader {
    ObjHeader {
        rc: AtomicU32::new(1),
        type_id,
    }
}

pub fn value_eq(a: Value, b: Value) -> bool {
    let mut seen: HashSet<(usize, usize)> = HashSet::new();
    value_eq_inner(a, b, 0, &mut seen)
}

fn value_eq_inner(a: Value, b: Value, depth: usize, seen: &mut HashSet<(usize, usize)>) -> bool {
    if depth > 64 {
        return false;
    }
    if primitive_eq(a, b) {
        return true;
    }
    if !(a.is_ptr() && b.is_ptr()) {
        return false;
    }
    let ap = a.as_ptr() as usize;
    let bp = b.as_ptr() as usize;
    let key = if ap <= bp { (ap, bp) } else { (bp, ap) };
    if !seen.insert(key) {
        return true;
    }
    unsafe {
        let ah = &*a.as_ptr();
        let bh = &*b.as_ptr();
        if ah.type_id != bh.type_id {
            return false;
        }
        if ah.type_id == TypeId::List as u32 {
            let Some(al) = crate::list::as_list_ref(a) else {
                return false;
            };
            let Some(bl) = crate::list::as_list_ref(b) else {
                return false;
            };
            if (*al).len != (*bl).len {
                return false;
            }
            for idx in 0..(*al).len {
                let av = (&(*al).data)[idx];
                let bv = (&(*bl).data)[idx];
                if !value_eq_inner(av, bv, depth + 1, seen) {
                    return false;
                }
            }
            return true;
        }
        if ah.type_id == TypeId::Map as u32 {
            let Some(am) = crate::map::as_map_ref(a) else {
                return false;
            };
            let Some(bm) = crate::map::as_map_ref(b) else {
                return false;
            };
            if crate::map::map_len(am) != crate::map::map_len(bm) {
                return false;
            }
            let mut iter = crate::map::map_iter(am);
            while let Some((map_key, map_value)) = iter.next() {
                let Some(other_value) = crate::map::map_get_raw(bm, map_key) else {
                    return false;
                };
                if !value_eq_inner(map_value, other_value, depth + 1, seen) {
                    return false;
                }
            }
            return true;
        }
        if ah.type_id == TypeId::Result as u32 {
            let Some((a_ok, a_val)) = crate::result::result_parts(a) else {
                return false;
            };
            let Some((b_ok, b_val)) = crate::result::result_parts(b) else {
                return false;
            };
            if a_ok != b_ok {
                return false;
            }
            return value_eq_inner(a_val, b_val, depth + 1, seen);
        }
        if ah.type_id >= TypeId::UserBase as u32 {
            let Some((a_type, a_fields)) = crate::class::class_type_and_fields(a) else {
                return false;
            };
            let Some((b_type, b_fields)) = crate::class::class_type_and_fields(b) else {
                return false;
            };
            if a_type != b_type || a_fields.len() != b_fields.len() {
                return false;
            }
            for ((a_name, a_value), (b_name, b_value)) in a_fields.iter().zip(b_fields.iter()) {
                if a_name != b_name {
                    return false;
                }
                if !value_eq_inner(*a_value, *b_value, depth + 1, seen) {
                    return false;
                }
            }
            return true;
        }
    }
    false
}

fn primitive_eq(a: Value, b: Value) -> bool {
    if a.0 == b.0 {
        if a.is_float() && b.is_float() && a.as_float().is_nan() {
            return false;
        }
        return true;
    }
    if let (Some(ai), Some(bi)) = (int_value(a), int_value(b)) {
        return ai == bi;
    }
    if a.is_bool() && b.is_bool() {
        return a.as_bool() == b.as_bool();
    }
    if a.is_nil() && b.is_nil() {
        return true;
    }
    if a.is_float() && b.is_float() {
        return a.as_float() == b.as_float();
    }
    if a.is_ptr() && b.is_ptr() {
        unsafe {
            let ah = &*a.as_ptr();
            let bh = &*b.as_ptr();
            if ah.type_id == TypeId::String as u32 && bh.type_id == TypeId::String as u32 {
                let eq =
                    with_string_bytes(a, |ab| with_string_bytes(b, |bb| ab == bb).unwrap_or(false));
                return eq.unwrap_or(false);
            }
            if ah.type_id == TypeId::Bytes as u32 && bh.type_id == TypeId::Bytes as u32 {
                let eq = with_bytes(a, |ab| with_bytes(b, |bb| ab == bb).unwrap_or(false));
                return eq.unwrap_or(false);
            }
        }
    }
    if let (Some(ai), true) = (int_value(a), b.is_float()) {
        let af = ai as f64;
        let bf = b.as_float();
        // Guard: only equal if the int→f64 cast is lossless (round-trip check).
        return af == bf && af as i64 == ai;
    }
    if let (Some(bi), true) = (int_value(b), a.is_float()) {
        let af = a.as_float();
        let bf = bi as f64;
        // Guard: only equal if the int→f64 cast is lossless (round-trip check).
        return af == bf && bf as i64 == bi;
    }
    false
}

pub fn value_hash<H: std::hash::Hasher>(val: Value, state: &mut H) {
    use std::hash::Hash;
    let mut seen = HashSet::new();
    value_hash_inner(val, 0, &mut seen).hash(state);
}

fn value_hash_inner(val: Value, depth: usize, seen: &mut HashSet<usize>) -> u64 {
    if depth > 64 {
        return 0x9e37_79b9_7f4a_7c15;
    }
    if let Some(i) = int_value(val) {
        return hash_tagged(1, i as u64);
    }
    if val.is_bool() {
        return hash_tagged(2, if val.as_bool() { 1 } else { 0 });
    }
    if val.is_nil() {
        return hash_tagged(3, 0);
    }
    if val.is_float() {
        let f = val.as_float();
        // Keep hash contract with int/float cross-equality.
        let i = f as i64;
        if i as f64 == f && !f.is_nan() {
            return hash_tagged(1, i as u64);
        }
        return hash_tagged(4, f.to_bits());
    }
    if !val.is_ptr() {
        return hash_tagged(0, val.0);
    }
    let ptr = val.as_ptr() as usize;
    if !seen.insert(ptr) {
        return hash_tagged(5, ptr as u64);
    }
    let hash = unsafe {
        let header = &*val.as_ptr();
        if header.type_id == TypeId::String as u32 {
            with_string_bytes(val, hash_bytes)
                .map(|h| hash_tagged(6, h))
                .unwrap_or_else(|| hash_tagged(6, 0))
        } else if header.type_id == TypeId::Bytes as u32 {
            with_bytes(val, hash_bytes)
                .map(|h| hash_tagged(7, h))
                .unwrap_or_else(|| hash_tagged(7, 0))
        } else if header.type_id == TypeId::List as u32 {
            if let Some(list) = crate::list::as_list_ref(val) {
                let mut acc = hash_tagged(8, (*list).len as u64);
                for idx in 0..(*list).len {
                    let item = (&(*list).data)[idx];
                    acc = hash_combine(acc, value_hash_inner(item, depth + 1, seen));
                }
                acc
            } else {
                hash_tagged(8, 0)
            }
        } else if header.type_id == TypeId::Map as u32 {
            if let Some(map) = crate::map::as_map_ref(val) {
                let mut pair_hashes = Vec::with_capacity(crate::map::map_len(map));
                let mut iter = crate::map::map_iter(map);
                while let Some((map_key, map_value)) = iter.next() {
                    let key_hash = value_hash_inner(map_key.0, depth + 1, seen);
                    let value_hash = value_hash_inner(map_value, depth + 1, seen);
                    pair_hashes.push(hash_combine(key_hash, value_hash));
                }
                pair_hashes.sort_unstable();
                let mut acc = hash_tagged(9, pair_hashes.len() as u64);
                for pair_hash in pair_hashes {
                    acc = hash_combine(acc, pair_hash);
                }
                acc
            } else {
                hash_tagged(9, 0)
            }
        } else if header.type_id == TypeId::Result as u32 {
            if let Some((is_ok, result_value)) = crate::result::result_parts(val) {
                hash_combine(
                    hash_tagged(10, if is_ok { 1 } else { 0 }),
                    value_hash_inner(result_value, depth + 1, seen),
                )
            } else {
                hash_tagged(10, 0)
            }
        } else if header.type_id >= TypeId::UserBase as u32 {
            if let Some((type_id, fields)) = crate::class::class_type_and_fields(val) {
                let mut acc = hash_tagged(11, type_id as u64);
                for (name, field_value) in fields {
                    let name_hash = hash_bytes(&name);
                    let field_hash = value_hash_inner(field_value, depth + 1, seen);
                    acc = hash_combine(acc, hash_combine(name_hash, field_hash));
                }
                acc
            } else {
                hash_tagged(11, 0)
            }
        } else {
            hash_tagged(12, val.0)
        }
    };
    seen.remove(&ptr);
    hash
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

fn hash_combine(left: u64, right: u64) -> u64 {
    left.rotate_left(27) ^ right.wrapping_mul(0x9e37_79b9_7f4a_7c15)
}

fn hash_tagged(tag: u64, value: u64) -> u64 {
    hash_combine(tag.wrapping_mul(0x517c_c1b7_2722_0a95), value)
}

#[repr(C)]
struct IntBox {
    header: ObjHeader,
    val: i64,
}

fn box_int(val: i64) -> Value {
    let obj = Box::new(IntBox {
        header: header(TypeId::BoxedInteger),
        val,
    });
    Value::from_ptr(Box::into_raw(obj) as *mut ObjHeader)
}

#[cfg(test)]
pub fn force_boxed_int(val: i64) -> Value {
    box_int(val)
}

pub fn int_value(val: Value) -> Option<i64> {
    if val.is_int() {
        return Some(val.as_int());
    }
    if val.is_ptr() {
        unsafe {
            let header = &*val.as_ptr();
            if header.type_id == TypeId::BoxedInteger as u32 {
                let boxed = val.as_ptr() as *const IntBox;
                return Some((*boxed).val);
            }
        }
    }
    None
}

#[allow(dead_code)]
pub fn is_int_value(val: Value) -> bool {
    int_value(val).is_some()
}

/// # Safety
/// `ptr` must be a valid `IntBox` allocation created by this runtime and must not be
/// concurrently or previously freed.
pub unsafe fn drop_boxed_int(ptr: *mut ObjHeader) {
    let boxed = ptr as *mut IntBox;
    unsafe {
        drop(Box::from_raw(boxed));
    }
}
