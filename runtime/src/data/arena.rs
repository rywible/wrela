use std::cell::Cell;
use std::collections::HashSet;
use std::mem::{align_of, size_of};
use std::sync::atomic::{AtomicU64, Ordering};

static ARENA_GENERATION: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static ARENA_THREAD_GENERATION: Cell<u64> = const { Cell::new(0) };
}

thread_local! {
    static ACTIVE_ARENA: Cell<*mut Arena> = const { Cell::new(std::ptr::null_mut()) };
}

#[derive(Clone, Copy)]
struct ObjAllocation {
    ptr: usize,
    layout: std::alloc::Layout,
    dealloc: bool,
}

#[derive(Default)]
pub struct Arena {
    allocations: Vec<ObjAllocation>,
    allocation_set: HashSet<usize>,
    obj_chunks: Vec<Chunk>,
    bytes_chunks: Vec<BytesChunk>,
    generation: u64,
}

struct Chunk {
    ptr: usize,
    cap: usize,
    used: usize,
    align: usize,
}

struct BytesChunk {
    ptr: usize,
    cap: usize,
    used: usize,
    align: usize,
}

impl Arena {
    pub fn new(_capacity: usize) -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        for allocation in self.allocations.drain(..) {
            unsafe {
                drop_object_in_arena(allocation.ptr as *mut crate::object::ObjHeader);
                if allocation.dealloc {
                    std::alloc::dealloc(allocation.ptr as *mut u8, allocation.layout);
                }
            }
        }
        self.allocation_set.clear();
        for chunk in self.obj_chunks.drain(..) {
            unsafe {
                let layout = std::alloc::Layout::from_size_align_unchecked(chunk.cap, chunk.align);
                std::alloc::dealloc(chunk.ptr as *mut u8, layout);
            }
        }
        for chunk in self.bytes_chunks.drain(..) {
            unsafe {
                let layout = std::alloc::Layout::from_size_align_unchecked(chunk.cap, chunk.align);
                std::alloc::dealloc(chunk.ptr as *mut u8, layout);
            }
        }
    }

    #[allow(dead_code)]
    pub fn live(&self) -> usize {
        self.allocations.len() + self.obj_chunks.len() + self.bytes_chunks.len()
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
    let arena_gen = ARENA_GENERATION.fetch_add(1, Ordering::Relaxed);
    unsafe {
        (*arena).generation = arena_gen;
    }
    ARENA_THREAD_GENERATION.with(|slot| slot.set(arena_gen));
    let previous = ACTIVE_ARENA.with(|slot| {
        let prev = slot.get();
        slot.set(arena);
        prev
    });
    ArenaGuard { previous }
}

pub fn alloc_in_current<T>(value: T) -> Option<*mut T> {
    ACTIVE_ARENA.with(|slot| {
        let arena_ptr = slot.get();
        if arena_ptr.is_null() {
            return None;
        }
        // Bump allocate objects in the arena to avoid per-object allocator overhead.
        // Destructors still run via `drop_object_in_arena` at reset; memory is reclaimed by
        // freeing the chunk (no per-object dealloc).
        let arena = unsafe { &mut *arena_ptr };
        let size = size_of::<T>().max(1);
        let align = align_of::<T>().max(1);
        if !align.is_power_of_two() {
            return None;
        }
        let ptr = alloc_from_chunks(&mut arena.obj_chunks, size, align)?;
        let raw = ptr as *mut T;
        unsafe {
            raw.write(value);
            arena.allocations.push(ObjAllocation {
                ptr,
                layout: std::alloc::Layout::new::<T>(),
                dealloc: false,
            });
            arena.allocation_set.insert(ptr);
        }
        Some(raw)
    })
}

fn alloc_from_chunks(chunks: &mut Vec<Chunk>, want: usize, align: usize) -> Option<usize> {
    debug_assert!(want > 0);
    debug_assert!(align.is_power_of_two());
    if let Some(chunk) = chunks.last_mut()
        && chunk.align >= align
    {
        let mask = align - 1;
        let aligned = (chunk.used + mask) & !mask;
        if aligned.saturating_add(want) <= chunk.cap {
            let out = chunk.ptr + aligned;
            chunk.used = aligned + want;
            return Some(out);
        }
    }
    const DEFAULT_CHUNK: usize = 64 * 1024;
    let cap = DEFAULT_CHUNK.max(want.next_power_of_two());
    let chunk_align = align.max(16);
    let layout = std::alloc::Layout::from_size_align(cap, chunk_align).ok()?;
    let ptr = unsafe { std::alloc::alloc(layout) } as usize;
    if ptr == 0 {
        return None;
    }
    let mut chunk = Chunk {
        ptr,
        cap,
        used: 0,
        align: chunk_align,
    };
    let mask = align - 1;
    let aligned = (chunk.used + mask) & !mask;
    let out = chunk.ptr + aligned;
    chunk.used = aligned + want;
    chunks.push(chunk);
    Some(out)
}

pub fn alloc_bytes_in_current(len: usize, align: usize) -> Option<*mut u8> {
    ACTIVE_ARENA.with(|slot| {
        let arena_ptr = slot.get();
        if arena_ptr.is_null() {
            return None;
        }
        // Fast bump allocation for arena-backed byte buffers (strings/bytes local temps).
        // Align is typically 1; keep this general but simple.
        let want = len.max(1);
        let align = align.max(1);
        if !align.is_power_of_two() {
            return None;
        }
        let arena = unsafe { &mut *arena_ptr };

        // Find an existing chunk with enough space.
        if let Some(chunk) = arena.bytes_chunks.last_mut()
            && chunk.align >= align
        {
            let mask = align - 1;
            let aligned = (chunk.used + mask) & !mask;
            if aligned.saturating_add(want) <= chunk.cap {
                let out = unsafe { (chunk.ptr as *mut u8).add(aligned) };
                chunk.used = aligned + want;
                return Some(out);
            }
        }

        // Allocate a new chunk.
        const DEFAULT_CHUNK: usize = 64 * 1024;
        let cap = DEFAULT_CHUNK.max(want.next_power_of_two());
        let chunk_align = align.max(16);
        let layout = std::alloc::Layout::from_size_align(cap, chunk_align).ok()?;
        let ptr = unsafe { std::alloc::alloc(layout) };
        if ptr.is_null() {
            return None;
        }
        let mut chunk = BytesChunk {
            ptr: ptr as usize,
            cap,
            used: 0,
            align: chunk_align,
        };
        let mask = align - 1;
        let aligned = (chunk.used + mask) & !mask;
        let out = unsafe { (chunk.ptr as *mut u8).add(aligned) };
        chunk.used = aligned + want;
        arena.bytes_chunks.push(chunk);
        Some(out)
    })
}

pub fn is_arena_value(val: crate::value::Value) -> bool {
    if !val.is_ptr() {
        return false;
    }
    is_arena_ptr(val.as_ptr())
}

pub fn is_arena_ptr(ptr: *mut crate::object::ObjHeader) -> bool {
    if ptr.is_null() {
        return false;
    }
    ACTIVE_ARENA.with(|slot| {
        let arena_ptr = slot.get();
        if arena_ptr.is_null() {
            return false;
        }
        let is_in_arena = unsafe { (*arena_ptr).allocation_set.contains(&(ptr as usize)) };
        if cfg!(debug_assertions) && is_in_arena {
            let arena_gen = unsafe { (*arena_ptr).generation };
            let thread_gen = ARENA_THREAD_GENERATION.with(|s| s.get());
            debug_assert_eq!(
                arena_gen, thread_gen,
                "arena pointer escaped its owning thread (arena gen={}, thread gen={})",
                arena_gen, thread_gen,
            );
        }
        is_in_arena
    })
}

pub fn drop_object_in_arena(ptr: *mut crate::object::ObjHeader) {
    if ptr.is_null() {
        return;
    }
    let type_id = unsafe { (*ptr).type_id };
    match type_id {
        x if x == crate::value::TypeId::String as u32 => crate::string::drop_string_in_arena(ptr),
        x if x == crate::value::TypeId::List as u32 => crate::list::drop_list_in_arena(ptr),
        x if x == crate::value::TypeId::Map as u32 => crate::map::drop_map_in_arena(ptr),
        x if x == crate::value::TypeId::Result as u32 => crate::result::drop_result_in_arena(ptr),
        x if x == crate::value::TypeId::Bytes as u32 => crate::bytes::drop_bytes_in_arena(ptr),
        _ => {
            if type_id >= crate::value::TypeId::UserBase as u32 {
                crate::class::drop_class_in_arena(ptr);
            }
        }
    }
}

pub fn reject_arena_escape(
    val: crate::value::Value,
    _context: &str,
) -> Option<crate::value::Value> {
    if is_arena_value(val) {
        return None;
    }
    Some(val)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_alloc_returns_non_null_and_is_writable() {
        let mut arena = Arena::new(0);
        let _guard = enter(&mut arena as *mut Arena);
        let ptr = alloc_bytes_in_current(128, 1).expect("alloc 128 bytes");
        assert!(!ptr.is_null());
        unsafe {
            std::ptr::write_bytes(ptr, 0xAB, 128);
            assert_eq!(*ptr, 0xAB);
        }
    }

    #[test]
    fn multiple_bytes_allocs_do_not_overlap() {
        let mut arena = Arena::new(0);
        let _guard = enter(&mut arena as *mut Arena);
        let a = alloc_bytes_in_current(64, 1).expect("alloc a") as usize;
        let b = alloc_bytes_in_current(64, 1).expect("alloc b") as usize;
        let range_a = a..a + 64;
        let range_b = b..b + 64;
        assert!(
            range_a.end <= range_b.start || range_b.end <= range_a.start,
            "allocations must not overlap"
        );
    }

    #[test]
    fn reset_clears_all_allocations() {
        let mut arena = Arena::new(0);
        {
            let _guard = enter(&mut arena as *mut Arena);
            alloc_bytes_in_current(1024, 1).expect("alloc");
        }
        assert!(arena.live() > 0);
        arena.reset();
        assert_eq!(arena.live(), 0);
    }

    #[test]
    fn no_active_arena_returns_none() {
        assert!(alloc_bytes_in_current(16, 1).is_none());
    }

    #[test]
    fn large_alloc_succeeds() {
        let mut arena = Arena::new(0);
        let _guard = enter(&mut arena as *mut Arena);
        let ptr = alloc_bytes_in_current(256 * 1024, 1).expect("large alloc");
        assert!(!ptr.is_null());
    }
}
