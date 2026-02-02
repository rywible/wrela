use crate::bytes::with_bytes;
use crate::object::ObjHeader;
use crate::string::with_string_bytes;
use std::sync::atomic::AtomicU32;

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Value(pub u64);

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
        if v < Self::MIN_INT || v > Self::MAX_INT {
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
    Int = 1,
    Bool = 2,
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
    BoxedInt = 14,
    UserBase = 100,
}

pub fn type_id_raw(val: Value) -> u32 {
    if val.is_int() {
        return TypeId::Int as u32;
    }
    if val.is_bool() {
        return TypeId::Bool as u32;
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
            if header.type_id == TypeId::BoxedInt as u32 {
                return TypeId::Int as u32;
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
        return af == bf;
    }
    if let (Some(bi), true) = (int_value(b), a.is_float()) {
        let af = a.as_float();
        let bf = bi as f64;
        return af == bf;
    }
    false
}

pub fn value_hash<H: std::hash::Hasher>(val: Value, state: &mut H) {
    use std::hash::Hash;
    if let Some(i) = int_value(val) {
        i.hash(state);
        return;
    }
    if val.is_bool() {
        val.as_bool().hash(state);
        return;
    }
    if val.is_nil() {
        0u8.hash(state);
        return;
    }
    if val.is_float() {
        val.as_float().to_bits().hash(state);
        return;
    }
    if val.is_ptr() {
        unsafe {
            let header = &*val.as_ptr();
            if header.type_id == TypeId::String as u32 {
                let _ = with_string_bytes(val, |bytes| {
                    bytes.hash(state);
                });
                return;
            }
            if header.type_id == TypeId::Bytes as u32 {
                let _ = with_bytes(val, |bytes| {
                    bytes.hash(state);
                });
                return;
            }
        }
    }
    val.0.hash(state);
}

#[repr(C)]
struct IntBox {
    header: ObjHeader,
    val: i64,
}

fn box_int(val: i64) -> Value {
    let obj = Box::new(IntBox {
        header: header(TypeId::BoxedInt),
        val,
    });
    Value::from_ptr(Box::into_raw(obj) as *mut ObjHeader)
}

pub fn int_value(val: Value) -> Option<i64> {
    if val.is_int() {
        return Some(val.as_int());
    }
    if val.is_ptr() {
        unsafe {
            let header = &*val.as_ptr();
            if header.type_id == TypeId::BoxedInt as u32 {
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

pub unsafe fn drop_boxed_int(ptr: *mut ObjHeader) {
    let boxed = ptr as *mut IntBox;
    unsafe {
        drop(Box::from_raw(boxed));
    }
}
