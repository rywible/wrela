use std::cell::Cell;

thread_local! {
    static ACTIVE_ARENA: Cell<*mut Arena> = const { Cell::new(std::ptr::null_mut()) };
}

#[derive(Default)]
pub struct Arena {
    allocations: Vec<usize>,
    bytes_allocations: Vec<(usize, usize, usize)>,
}

impl Arena {
    pub fn new(_capacity: usize) -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        for ptr in self.allocations.drain(..) {
            unsafe {
                drop(Box::from_raw(ptr as *mut u8));
            }
        }
        for (ptr, len, align) in self.bytes_allocations.drain(..) {
            unsafe {
                let layout =
                    std::alloc::Layout::from_size_align_unchecked(len.max(1), align.max(1));
                std::alloc::dealloc(ptr as *mut u8, layout);
            }
        }
    }

    pub fn live(&self) -> usize {
        self.allocations.len() + self.bytes_allocations.len()
    }
}

impl Drop for Arena {
    fn drop(&mut self) {
        self.reset();
    }
}

pub struct ArenaGuard {
    previous: *mut Arena,
}

impl Drop for ArenaGuard {
    fn drop(&mut self) {
        ACTIVE_ARENA.with(|slot| slot.set(self.previous));
    }
}

pub fn enter(arena: *mut Arena) -> ArenaGuard {
    let previous = ACTIVE_ARENA.with(|slot| {
        let prev = slot.get();
        slot.set(arena);
        prev
    });
    ArenaGuard { previous }
}

pub fn reset_current() {
    ACTIVE_ARENA.with(|slot| {
        let ptr = slot.get();
        if ptr.is_null() {
            return;
        }
        unsafe {
            (*ptr).reset();
        }
    });
}

pub fn alloc_in_current<T>(value: T) -> Option<*mut T> {
    ACTIVE_ARENA.with(|slot| {
        let arena_ptr = slot.get();
        if arena_ptr.is_null() {
            return None;
        }
        let boxed = Box::new(value);
        let raw = Box::into_raw(boxed);
        unsafe {
            (*arena_ptr).allocations.push(raw as usize);
        }
        Some(raw)
    })
}

pub fn alloc_bytes_in_current(len: usize, align: usize) -> Option<*mut u8> {
    ACTIVE_ARENA.with(|slot| {
        let arena_ptr = slot.get();
        if arena_ptr.is_null() {
            return None;
        }
        let layout = std::alloc::Layout::from_size_align(len.max(1), align.max(1)).ok()?;
        let ptr = unsafe { std::alloc::alloc(layout) };
        if ptr.is_null() {
            return None;
        }
        unsafe {
            (*arena_ptr)
                .bytes_allocations
                .push((ptr as usize, len.max(1), align.max(1)));
        }
        Some(ptr)
    })
}

pub fn is_arena_value(_val: crate::value::Value) -> bool {
    false
}

pub fn is_arena_ptr(_ptr: *mut crate::object::ObjHeader) -> bool {
    false
}

pub fn drop_object_in_arena(_ptr: *mut crate::object::ObjHeader) {
    // Arena-backed object tracking is disabled in this trimmed runtime.
}

pub fn reject_arena_escape(
    val: crate::value::Value,
    _context: &str,
) -> Option<crate::value::Value> {
    Some(val)
}
