use crate::object::ObjHeader;
use crate::string::with_string_bytes;
use std::sync::atomic::AtomicU32;

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Value(pub u64);

impl Value {
    pub const TAG_PTR: u64 = 0b000;
    pub const TAG_INT: u64 = 0b001;
    pub const TAG_BOOL: u64 = 0b010;
    pub const TAG_NIL: u64 = 0b011;
    pub const TAG_MASK: u64 = 0b111;

    #[inline]
    pub const fn nil() -> Self {
        Value(Self::TAG_NIL)
    }

    #[inline]
    pub const fn from_int(v: i64) -> Self {
        Value(((v as u64) << 3) | Self::TAG_INT)
    }

    #[inline]
    pub const fn from_bool(v: bool) -> Self {
        Value(((v as u64) << 3) | Self::TAG_BOOL)
    }

    #[inline]
    pub fn from_ptr(ptr: *mut ObjHeader) -> Self {
        debug_assert!(!ptr.is_null());
        let raw = ptr as u64;
        debug_assert_eq!(raw & Self::TAG_MASK, 0);
        Value(raw)
    }

    #[inline]
    pub const fn is_ptr(self) -> bool {
        (self.0 & Self::TAG_MASK) == Self::TAG_PTR && self.0 != 0
    }

    #[inline]
    pub const fn is_int(self) -> bool {
        (self.0 & Self::TAG_MASK) == Self::TAG_INT
    }

    #[inline]
    pub const fn is_bool(self) -> bool {
        (self.0 & Self::TAG_MASK) == Self::TAG_BOOL
    }

    #[inline]
    pub const fn is_nil(self) -> bool {
        (self.0 & Self::TAG_MASK) == Self::TAG_NIL
    }

    #[inline]
    pub const fn as_int(self) -> i64 {
        (self.0 as i64) >> 3
    }

    #[inline]
    pub const fn as_bool(self) -> bool {
        ((self.0 >> 3) & 1) != 0
    }

    #[inline]
    pub const fn as_ptr(self) -> *mut ObjHeader {
        (self.0 & !Self::TAG_MASK) as *mut ObjHeader
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
    if val.is_ptr() {
        unsafe {
            let header = &*val.as_ptr();
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
        return true;
    }
    if a.is_int() && b.is_int() {
        return a.as_int() == b.as_int();
    }
    if a.is_bool() && b.is_bool() {
        return a.as_bool() == b.as_bool();
    }
    if a.is_nil() && b.is_nil() {
        return true;
    }
    if a.is_ptr() && b.is_ptr() {
        unsafe {
            let ah = &*a.as_ptr();
            let bh = &*b.as_ptr();
            if ah.type_id == TypeId::Float as u32 && bh.type_id == TypeId::Float as u32 {
                let af = crate::float_box::unbox_float(a);
                let bf = crate::float_box::unbox_float(b);
                return af == bf;
            }
            if ah.type_id == TypeId::String as u32 && bh.type_id == TypeId::String as u32 {
                let eq = with_string_bytes(a, |ab| {
                    with_string_bytes(b, |bb| ab == bb).unwrap_or(false)
                });
                return eq.unwrap_or(false);
            }
        }
    }
    if a.is_int() && is_float_value(b) {
        let af = a.as_int() as f64;
        let bf = crate::float_box::unbox_float(b);
        return af == bf;
    }
    if b.is_int() && is_float_value(a) {
        let af = crate::float_box::unbox_float(a);
        let bf = b.as_int() as f64;
        return af == bf;
    }
    false
}

pub fn value_hash<H: std::hash::Hasher>(val: Value, state: &mut H) {
    use std::hash::Hash;
    if val.is_int() {
        val.as_int().hash(state);
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
    if val.is_ptr() {
        unsafe {
            let header = &*val.as_ptr();
            if header.type_id == TypeId::Float as u32 {
                let f = crate::float_box::unbox_float(val);
                f.to_bits().hash(state);
                return;
            }
            if header.type_id == TypeId::String as u32 {
                let _ = with_string_bytes(val, |bytes| {
                    bytes.hash(state);
                });
                return;
            }
        }
    }
    val.0.hash(state);
}

fn is_float_value(val: Value) -> bool {
    if !val.is_ptr() {
        return false;
    }
    unsafe { (*val.as_ptr()).type_id == TypeId::Float as u32 }
}
