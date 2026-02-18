#![allow(dead_code)]

use std::marker::PhantomData;
use std::sync::atomic::Ordering;

pub const RUNTIME_CAP_ATOMIC_ORDERINGS: u64 = 1 << 0;
pub const RUNTIME_CAP_OPAQUE_POINTERS: u64 = 1 << 1;
pub const RUNTIME_CAP_BOX_BRIDGE: u64 = 1 << 2;
pub const RUNTIME_CAP_FFI_LAYOUT: u64 = 1 << 3;
pub const RUNTIME_CAP_DETERMINISTIC_TIME_HOOKS: u64 = 1 << 4;
pub const RUNTIME_CAP_ABI_NEGOTIATION_MARKER: u64 = 1 << 5;

pub const fn runtime_caps_mask() -> u64 {
    RUNTIME_CAP_ATOMIC_ORDERINGS
        | RUNTIME_CAP_OPAQUE_POINTERS
        | RUNTIME_CAP_BOX_BRIDGE
        | RUNTIME_CAP_FFI_LAYOUT
        | RUNTIME_CAP_DETERMINISTIC_TIME_HOOKS
        | RUNTIME_CAP_ABI_NEGOTIATION_MARKER
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pointer<T> {
    raw: *mut T,
    _marker: PhantomData<T>,
}

impl<T> Pointer<T> {
    pub const fn null() -> Self {
        Self {
            raw: std::ptr::null_mut(),
            _marker: PhantomData,
        }
    }

    pub const fn from_raw(raw: *mut T) -> Self {
        Self {
            raw,
            _marker: PhantomData,
        }
    }

    pub const fn as_raw(self) -> *mut T {
        self.raw
    }

    pub const fn is_null(self) -> bool {
        self.raw.is_null()
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NonNullPointer<T> {
    raw: std::ptr::NonNull<T>,
}

impl<T> NonNullPointer<T> {
    pub fn from_raw(raw: *mut T) -> Option<Self> {
        std::ptr::NonNull::new(raw).map(|raw| Self { raw })
    }

    pub const fn as_raw(self) -> *mut T {
        self.raw.as_ptr()
    }
}

#[repr(transparent)]
pub struct RuntimeBox<T>(Box<T>);

impl<T> RuntimeBox<T> {
    pub fn new(value: T) -> Self {
        Self(Box::new(value))
    }

    pub fn into_pointer(self) -> NonNullPointer<T> {
        let ptr = Box::into_raw(self.0);
        NonNullPointer::from_raw(ptr).expect("Box::into_raw is never null")
    }

    pub unsafe fn from_pointer(pointer: NonNullPointer<T>) -> Self {
        Self(unsafe { Box::from_raw(pointer.as_raw()) })
    }

    pub fn as_ref(&self) -> &T {
        &self.0
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtomicMemoryOrder {
    Relaxed = 0,
    Acquire = 1,
    Release = 2,
    AcqRel = 3,
    SeqCst = 4,
}

impl AtomicMemoryOrder {
    pub const fn as_ordering(self) -> Ordering {
        match self {
            Self::Relaxed => Ordering::Relaxed,
            Self::Acquire => Ordering::Acquire,
            Self::Release => Ordering::Release,
            Self::AcqRel => Ordering::AcqRel,
            Self::SeqCst => Ordering::SeqCst,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_caps_mask_exposes_expected_flags() {
        let caps = runtime_caps_mask();
        assert_ne!(caps & RUNTIME_CAP_ATOMIC_ORDERINGS, 0);
        assert_ne!(caps & RUNTIME_CAP_OPAQUE_POINTERS, 0);
        assert_ne!(caps & RUNTIME_CAP_BOX_BRIDGE, 0);
        assert_ne!(caps & RUNTIME_CAP_FFI_LAYOUT, 0);
        assert_ne!(caps & RUNTIME_CAP_DETERMINISTIC_TIME_HOOKS, 0);
        assert_ne!(caps & RUNTIME_CAP_ABI_NEGOTIATION_MARKER, 0);
    }

    #[test]
    fn runtime_box_roundtrip_preserves_value() {
        let boxed = RuntimeBox::new(42_i64);
        let pointer = boxed.into_pointer();
        let restored = unsafe { RuntimeBox::from_pointer(pointer) };
        assert_eq!(*restored.as_ref(), 42_i64);
    }

    #[test]
    fn pointer_and_non_null_pointer_helpers_work() {
        let mut value = 7_i64;
        let pointer = Pointer::from_raw(&mut value as *mut i64);
        assert!(!pointer.is_null());
        let non_null_pointer = NonNullPointer::from_raw(pointer.as_raw()).expect("non-null");
        assert_eq!(non_null_pointer.as_raw(), pointer.as_raw());
    }
}
