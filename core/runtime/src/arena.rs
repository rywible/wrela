use crate::object::ObjHeader;
use crate::value::{TypeId, Value};
use crate::{wr_rc_dec};
use std::cell::Cell;

pub struct Arena {
    buf: Vec<u8>,
    offset: usize,
    live: usize,
}

impl Arena {
    pub fn new(capacity: usize) -> Self {
        Self {
            buf: Vec::with_capacity(capacity),
            offset: 0,
            live: 0,
        }
    }

    pub fn reset(&mut self) {
        self.buf.clear();
        self.offset = 0;
        self.live = 0;
    }

    pub fn live(&self) -> usize {
        self.live
    }

    fn alloc_bytes(&mut self, size: usize, align: usize) -> *mut u8 {
        let align_mask = align.saturating_sub(1);
        let mut offset = self.offset;
        if align_mask != 0 {
            offset = (offset + align_mask) & !align_mask;
        }
        let end = offset.saturating_add(size);
        if end > self.buf.capacity() {
            let extra = end.saturating_sub(self.buf.capacity());
            self.buf.reserve(extra.max(1024));
        }
        if end > self.buf.len() {
            unsafe {
                self.buf.set_len(end);
            }
        }
        self.offset = end;
        unsafe { self.buf.as_mut_ptr().add(offset) }
    }

    pub fn alloc_obj<T>(&mut self, value: T) -> *mut T {
        let ptr = self.alloc_bytes(std::mem::size_of::<T>(), std::mem::align_of::<T>());
        unsafe {
            let out = ptr as *mut T;
            out.write(value);
            self.live = self.live.saturating_add(1);
            out
        }
    }

    pub fn span(&self) -> (usize, usize) {
        let start = self.buf.as_ptr() as usize;
        let end = unsafe { self.buf.as_ptr().add(self.buf.len()) } as usize;
        (start, end)
    }

    pub fn dec_live(&mut self) {
        if self.live > 0 {
            self.live -= 1;
        }
    }
}

thread_local! {
    static CURRENT_ARENA: Cell<*mut Arena> = Cell::new(std::ptr::null_mut());
}

pub struct ArenaGuard {
    prev: *mut Arena,
}

impl Drop for ArenaGuard {
    fn drop(&mut self) {
        CURRENT_ARENA.with(|cell| cell.set(self.prev));
    }
}

pub fn enter(arena: *mut Arena) -> ArenaGuard {
    let prev = CURRENT_ARENA.with(|cell| {
        let prev = cell.get();
        cell.set(arena);
        prev
    });
    ArenaGuard { prev }
}

pub fn set_current(arena: *mut Arena) -> *mut Arena {
    CURRENT_ARENA.with(|cell| {
        let prev = cell.get();
        cell.set(arena);
        prev
    })
}

pub fn new_arena(capacity: usize) -> *mut Arena {
    Box::into_raw(Box::new(Arena::new(capacity)))
}

pub fn current() -> Option<*mut Arena> {
    CURRENT_ARENA.with(|cell| {
        let ptr = cell.get();
        if ptr.is_null() {
            None
        } else {
            Some(ptr)
        }
    })
}

pub fn alloc_in_current<T>(value: T) -> Option<*mut T> {
    let arena = current()?;
    unsafe { Some((&mut *arena).alloc_obj(value)) }
}

pub fn alloc_bytes_in_current(size: usize, align: usize) -> Option<*mut u8> {
    let arena = current()?;
    unsafe { Some((&mut *arena).alloc_bytes(size, align)) }
}

pub fn is_arena_ptr(ptr: *const ObjHeader) -> bool {
    let Some(arena) = current() else {
        return false;
    };
    let (start, end) = unsafe { (&*arena).span() };
    let addr = ptr as usize;
    addr >= start && addr < end
}

pub fn is_arena_value(val: Value) -> bool {
    val.is_ptr() && is_arena_ptr(val.as_ptr())
}

pub fn reset_current() {
    if let Some(arena) = current() {
        unsafe { (&mut *arena).reset() };
    }
}

pub fn current_live() -> Option<usize> {
    current().map(|arena| unsafe { (&*arena).live() })
}

pub fn drop_object_in_arena(ptr: *mut ObjHeader) {
    if ptr.is_null() {
        return;
    }
    let type_id = unsafe { (*ptr).type_id };
    match type_id {
        x if x == TypeId::String as u32 => {
            crate::string::drop_string_in_arena(ptr);
        }
        x if x == TypeId::List as u32 => {
            crate::list::drop_list_in_arena(ptr);
        }
        x if x == TypeId::Map as u32 => {
            crate::map::drop_map_in_arena(ptr);
        }
        x if x == TypeId::Bytes as u32 => {
            crate::bytes::drop_bytes_in_arena(ptr);
        }
        x if x == TypeId::Result as u32 => {
            crate::result::drop_result_in_arena(ptr);
        }
        _ => {
            if type_id >= TypeId::UserBase as u32 {
                crate::class::drop_class_in_arena(ptr);
            }
        }
    }
    if let Some(arena) = current() {
        unsafe { (&mut *arena).dec_live() };
    }
}

pub fn reject_arena_escape(val: Value, context: &str) -> Option<Value> {
    if !is_arena_value(val) {
        return Some(val);
    }
    if crate::config::debug_actor_enabled() {
        eprintln!("arena: rejected escaping value in {}", context);
    }
    unsafe { wr_rc_dec(val) };
    Some(Value::nil())
}
