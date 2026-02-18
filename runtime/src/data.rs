pub(crate) mod arena {
    use std::cell::Cell;
    use std::collections::HashSet;
    use std::mem::{align_of, size_of};

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
                    let layout =
                        std::alloc::Layout::from_size_align_unchecked(chunk.cap, chunk.align);
                    std::alloc::dealloc(chunk.ptr as *mut u8, layout);
                }
            }
            for chunk in self.bytes_chunks.drain(..) {
                unsafe {
                    let layout =
                        std::alloc::Layout::from_size_align_unchecked(chunk.cap, chunk.align);
                    std::alloc::dealloc(chunk.ptr as *mut u8, layout);
                }
            }
        }

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
            unsafe { (*arena_ptr).allocation_set.contains(&(ptr as usize)) }
        })
    }

    pub fn drop_object_in_arena(ptr: *mut crate::object::ObjHeader) {
        if ptr.is_null() {
            return;
        }
        let type_id = unsafe { (*ptr).type_id };
        match type_id {
            x if x == crate::value::TypeId::String as u32 => {
                crate::string::drop_string_in_arena(ptr)
            }
            x if x == crate::value::TypeId::List as u32 => crate::list::drop_list_in_arena(ptr),
            x if x == crate::value::TypeId::Map as u32 => crate::map::drop_map_in_arena(ptr),
            x if x == crate::value::TypeId::Result as u32 => {
                crate::result::drop_result_in_arena(ptr)
            }
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
}

pub(crate) mod bytes {
    use crate::arena;
    use crate::list;
    #[cfg(feature = "metrics")]
    use crate::metrics::inc_alloc_bytes;
    use crate::object::ObjHeader;
    use crate::string;
    use crate::value::{TypeId, Value, header, int_value};

    #[repr(C)]
    pub struct BytesObj {
        header: ObjHeader,
        bytes: Vec<u8>,
        arena_backed: bool,
    }

    pub fn bytes_from_slice(bytes: &[u8]) -> Value {
        let obj = Box::new(BytesObj {
            header: header(TypeId::Bytes),
            bytes: bytes.to_vec(),
            arena_backed: false,
        });
        #[cfg(feature = "metrics")]
        inc_alloc_bytes();
        Value::from_ptr(Box::into_raw(obj) as *mut ObjHeader)
    }

    pub fn bytes_from_slice_local(bytes: &[u8]) -> Value {
        if let Some(bytes_ptr) = arena::alloc_bytes_in_current(bytes.len(), 1) {
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), bytes_ptr, bytes.len());
                let vec = Vec::from_raw_parts(bytes_ptr, bytes.len(), bytes.len());
                let obj = BytesObj {
                    header: header(TypeId::Bytes),
                    bytes: vec,
                    arena_backed: true,
                };
                if let Some(ptr) = arena::alloc_in_current(obj) {
                    #[cfg(feature = "metrics")]
                    inc_alloc_bytes();
                    return Value::from_ptr(ptr as *mut ObjHeader);
                }
            }
        }
        bytes_from_slice(bytes)
    }

    pub fn bytes_from_string(val: Value) -> Value {
        string::with_string_bytes(val, bytes_from_slice).unwrap_or(Value::nil())
    }

    pub fn bytes_to_string(val: Value) -> Value {
        with_bytes(val, |bytes| match std::str::from_utf8(bytes) {
            Ok(_) => string::str_from_bytes(bytes),
            Err(_) => Value::nil(),
        })
        .unwrap_or(Value::nil())
    }

    pub fn bytes_len(val: Value) -> Value {
        with_bytes(val, |bytes| Value::from_int(bytes.len() as i64)).unwrap_or(Value::nil())
    }

    #[allow(dead_code)]
    pub fn bytes_from_list(list_val: Value) -> Value {
        let list = match list::as_list_ref(list_val) {
            Some(list) => list,
            None => return Value::nil(),
        };
        let len = unsafe { (*list).len };
        let mut out = Vec::with_capacity(len);
        unsafe {
            for val in (*list).data.iter().take(len) {
                let Some(i) = int_value(*val) else {
                    return Value::nil();
                };
                if !(0..=255).contains(&i) {
                    return Value::nil();
                }
                out.push(i as u8);
            }
        }
        bytes_from_slice(&out)
    }

    #[allow(dead_code)]
    pub fn bytes_to_list(bytes_val: Value) -> Value {
        let bytes = match with_bytes(bytes_val, |bytes| bytes.to_vec()) {
            Some(bytes) => bytes,
            None => return Value::nil(),
        };
        let list = list::list_new(bytes.len());
        for (idx, byte) in bytes.iter().enumerate() {
            list::list_set(list, idx, Value::from_int(*byte as i64));
        }
        list
    }

    pub fn drop_bytes(ptr: *mut ObjHeader) {
        let bytes = ptr as *mut BytesObj;
        unsafe {
            drop(Box::from_raw(bytes));
        }
    }

    pub fn drop_bytes_in_arena(ptr: *mut ObjHeader) {
        let bytes = ptr as *mut BytesObj;
        unsafe {
            if !(*bytes).arena_backed {
                std::ptr::drop_in_place(&mut (*bytes).bytes);
            }
        }
    }

    pub fn with_bytes<F, R>(val: Value, f: F) -> Option<R>
    where
        F: FnOnce(&[u8]) -> R,
    {
        if !val.is_ptr() {
            return None;
        }
        unsafe {
            let header = &*val.as_ptr();
            if header.type_id != TypeId::Bytes as u32 {
                return None;
            }
            let bytes = &*(val.as_ptr() as *const BytesObj);
            Some(f(&bytes.bytes))
        }
    }

    #[allow(dead_code)]
    pub fn bytes_eq(a: Value, b: Value) -> Option<bool> {
        with_bytes(a, |ab| with_bytes(b, |bb| ab == bb)).and_then(|v| v)
    }

    #[allow(dead_code)]
    pub fn bytes_hash<H: std::hash::Hasher>(val: Value, state: &mut H) -> bool {
        use std::hash::Hash;
        if let Some(bytes) = with_bytes(val, |bytes| bytes.to_vec()) {
            bytes.hash(state);
            return true;
        }
        false
    }
}

pub(crate) mod class {
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
            if let Some(old) = (*obj).overflow.insert(key.to_vec(), val) {
                wr_rc_dec(old);
            }
            wr_rc_inc(val);
        }
    }

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

    fn to_slot_index(slot: u32, len: usize) -> Option<usize> {
        if slot == INVALID_SLOT {
            return None;
        }
        let index = slot as usize;
        if index < len { Some(index) } else { None }
    }
}

pub(crate) mod iter {
    use crate::list::as_list_ref;
    use crate::map::{
        MapIter, as_map_ref, map_inline_entry, map_inline_len, map_is_inline, map_iter, map_version,
    };
    use crate::object::ObjHeader;
    use crate::value::{TypeId, Value, header};
    use crate::{wr_rc_dec, wr_rc_inc};

    #[repr(C)]
    pub struct IterObj {
        header: ObjHeader,
        kind: IterKind,
    }

    #[repr(C)]
    pub enum IterKind {
        List {
            list: Value,
            index: usize,
        },
        Map {
            map: Value,
            index: usize,
            iter: Option<MapIter<'static>>,
            version: u64,
        },
    }

    pub fn iter_init(iterable: Value) -> Value {
        if let Some(_list) = as_list_ref(iterable) {
            unsafe { wr_rc_inc(iterable) };
            let obj = Box::new(IterObj {
                header: header(TypeId::Iterator),
                kind: IterKind::List {
                    list: iterable,
                    index: 0,
                },
            });
            return Value::from_ptr(Box::into_raw(obj) as *mut ObjHeader);
        }
        if let Some(map) = as_map_ref(iterable) {
            unsafe { wr_rc_inc(iterable) };
            let version = map_version(map);
            let iter = if map_is_inline(map) {
                None
            } else {
                Some(map_iter(map))
            };
            let obj = Box::new(IterObj {
                header: header(TypeId::Iterator),
                kind: IterKind::Map {
                    map: iterable,
                    index: 0,
                    iter,
                    version,
                },
            });
            return Value::from_ptr(Box::into_raw(obj) as *mut ObjHeader);
        }
        Value::nil()
    }

    pub fn iter_next(iter_val: Value, dst_value: *mut Value, dst_done: *mut Value) {
        if dst_value.is_null() || dst_done.is_null() {
            return;
        }
        unsafe {
            *dst_done = Value::from_bool(true);
            *dst_value = Value::nil();
        }
        if !iter_val.is_ptr() {
            return;
        }
        unsafe {
            let header = &*iter_val.as_ptr();
            if header.type_id != TypeId::Iterator as u32 {
                return;
            }
            let iter = &mut *(iter_val.as_ptr() as *mut IterObj);
            match &mut iter.kind {
                IterKind::List { list, index } => {
                    if let Some(list_ptr) = as_list_ref(*list)
                        && *index < (*list_ptr).len
                    {
                        let val = (&(*list_ptr).data)[*index];
                        wr_rc_inc(val);
                        *index += 1;
                        *dst_value = val;
                        *dst_done = Value::from_bool(false);
                    }
                }
                IterKind::Map {
                    map,
                    index,
                    iter,
                    version,
                } => {
                    let map_ptr = match as_map_ref(*map) {
                        Some(map_ptr) => map_ptr,
                        None => return,
                    };
                    if map_version(map_ptr) != *version {
                        return;
                    }
                    if map_is_inline(map_ptr) {
                        let len = map_inline_len(map_ptr);
                        if *index < len
                            && let Some((key, _)) = map_inline_entry(map_ptr, *index)
                        {
                            wr_rc_inc(key.0);
                            *index += 1;
                            *dst_value = key.0;
                            *dst_done = Value::from_bool(false);
                        }
                    } else if let Some(iter) = iter.as_mut()
                        && let Some((key, _)) = iter.next()
                    {
                        wr_rc_inc(key.0);
                        *dst_value = key.0;
                        *dst_done = Value::from_bool(false);
                    }
                }
            }
        }
    }

    pub unsafe fn drop_iter(ptr: *mut ObjHeader) {
        let iter = ptr as *mut IterObj;
        unsafe {
            match &mut (*iter).kind {
                IterKind::List { list, .. } => {
                    wr_rc_dec(*list);
                }
                IterKind::Map { map, .. } => {
                    wr_rc_dec(*map);
                }
            }
            drop(Box::from_raw(iter));
        }
    }
}

pub(crate) mod list {
    use crate::arena;
    #[cfg(feature = "metrics")]
    use crate::metrics::inc_alloc_list;
    use crate::object::ObjHeader;
    use crate::value::{TypeId, Value, header};
    use crate::{wr_rc_dec, wr_rc_inc};

    const FLAG_MUTABLE: u32 = 1;

    #[repr(C)]
    pub struct ListObj {
        pub(crate) header: ObjHeader,
        pub(crate) len: usize,
        pub(crate) cap: usize,
        pub(crate) data: Vec<Value>,
        pub(crate) flags: u32,
    }

    pub fn list_new(len: usize) -> Value {
        let mut data = Vec::with_capacity(len);
        data.resize(len, Value::nil());
        let obj = Box::new(ListObj {
            header: header(TypeId::List),
            len,
            cap: len,
            data,
            flags: FLAG_MUTABLE,
        });
        #[cfg(feature = "metrics")]
        inc_alloc_list();
        Value::from_ptr(Box::into_raw(obj) as *mut ObjHeader)
    }

    pub fn list_new_local(len: usize) -> Value {
        let mut data = Vec::with_capacity(len);
        data.resize(len, Value::nil());
        let obj = ListObj {
            header: header(TypeId::List),
            len,
            cap: len,
            data,
            flags: FLAG_MUTABLE,
        };
        if let Some(ptr) = arena::alloc_in_current(obj) {
            #[cfg(feature = "metrics")]
            inc_alloc_list();
            return Value::from_ptr(ptr as *mut ObjHeader);
        }
        list_new(len)
    }

    pub fn list_get(list_val: Value, idx: usize) -> Value {
        let list = match as_list_ref(list_val) {
            Some(list) => list,
            None => return Value::nil(),
        };
        unsafe {
            if idx >= (*list).len {
                return Value::nil();
            }
            let val = (&(*list).data)[idx];
            wr_rc_inc(val);
            val
        }
    }

    pub fn list_set(list_val: Value, idx: usize, val: Value) {
        let list = match as_list_ref(list_val) {
            Some(list) => list,
            None => return,
        };
        unsafe {
            if (*list).flags & FLAG_MUTABLE == 0 {
                return;
            }
            if idx >= (*list).len {
                return;
            }
            let old = (&(*list).data)[idx];
            (&mut (*list).data)[idx] = val;
            wr_rc_inc(val);
            wr_rc_dec(old);
        }
    }

    pub fn list_push(list_val: Value, val: Value) {
        let list = match as_list_ref(list_val) {
            Some(list) => list,
            None => return,
        };
        unsafe {
            if (*list).flags & FLAG_MUTABLE == 0 {
                return;
            }
            (*list).data.push(val);
            (*list).len += 1;
            (*list).cap = (*list).data.capacity();
            wr_rc_inc(val);
        }
    }

    pub fn list_len(list_val: Value) -> Value {
        let list = match as_list_ref(list_val) {
            Some(list) => list,
            None => return Value::nil(),
        };
        unsafe { Value::from_int((*list).len as i64) }
    }

    pub fn drop_list(ptr: *mut ObjHeader) {
        let list = ptr as *mut ListObj;
        unsafe {
            for val in (*list).data.iter() {
                wr_rc_dec(*val);
            }
            drop(Box::from_raw(list));
        }
    }

    pub fn drop_list_in_arena(ptr: *mut ObjHeader) {
        let list = ptr as *mut ListObj;
        unsafe {
            for val in (*list).data.iter() {
                wr_rc_dec(*val);
            }
            std::ptr::drop_in_place(&mut (*list).data);
        }
    }

    pub(crate) fn as_list_ref(val: Value) -> Option<*mut ListObj> {
        if !val.is_ptr() {
            return None;
        }
        unsafe {
            let header = &*val.as_ptr();
            if header.type_id != TypeId::List as u32 {
                return None;
            }
        }
        Some(val.as_ptr() as *mut ListObj)
    }
}

pub(crate) mod map {
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
}

pub(crate) mod object {
    use crate::actor::{drop_actor, drop_pending, drop_pool};
    use crate::bytes::drop_bytes;
    use crate::class::drop_class;
    use crate::iter::drop_iter;
    use crate::list::drop_list;
    use crate::map::drop_map;
    use crate::result::drop_result;
    use crate::string::drop_string;
    use crate::value::{TypeId, drop_boxed_int};
    use std::sync::atomic::AtomicU32;

    #[repr(C)]
    pub struct ObjHeader {
        pub rc: AtomicU32,
        pub type_id: u32,
    }

    pub unsafe fn drop_object(ptr: *mut ObjHeader) {
        if ptr.is_null() {
            return;
        }
        let type_id = unsafe { (*ptr).type_id };
        match type_id {
            x if x == TypeId::String as u32 => drop_string(ptr),
            x if x == TypeId::List as u32 => drop_list(ptr),
            x if x == TypeId::Map as u32 => drop_map(ptr),
            x if x == TypeId::Actor as u32 => unsafe { drop_actor(ptr) },
            x if x == TypeId::Pending as u32 => unsafe { drop_pending(ptr) },
            x if x == TypeId::Iterator as u32 => unsafe { drop_iter(ptr) },
            x if x == TypeId::Result as u32 => unsafe { drop_result(ptr) },
            x if x == TypeId::Pool as u32 => unsafe { drop_pool(ptr) },
            x if x == TypeId::Bytes as u32 => drop_bytes(ptr),
            x if x == TypeId::BoxedInteger as u32 => unsafe { drop_boxed_int(ptr) },
            _ => {
                if type_id >= TypeId::UserBase as u32 {
                    unsafe { drop_class(ptr) };
                }
            }
        }
    }
}

pub(crate) mod result {
    use crate::arena;
    #[cfg(feature = "metrics")]
    use crate::metrics::inc_alloc_result;
    use crate::object::ObjHeader;
    use crate::value::{TypeId, Value, header};
    use crate::{wr_rc_dec, wr_rc_inc};

    #[repr(C)]
    struct ResultObj {
        header: ObjHeader,
        is_ok: u64,
        value: Value,
    }

    pub fn result_ok(value: Value) -> Value {
        if arena::is_arena_value(value) {
            if crate::config::debug_actor_enabled() {
                eprintln!("arena: rejected Result.ok with arena-backed value");
            }
            return Value::nil();
        }
        unsafe { wr_rc_inc(value) };
        let obj = Box::new(ResultObj {
            header: header(TypeId::Result),
            is_ok: 1,
            value,
        });
        #[cfg(feature = "metrics")]
        inc_alloc_result();
        Value::from_ptr(Box::into_raw(obj) as *mut ObjHeader)
    }

    pub fn result_err(value: Value) -> Value {
        if arena::is_arena_value(value) {
            if crate::config::debug_actor_enabled() {
                eprintln!("arena: rejected Result.err with arena-backed value");
            }
            return Value::nil();
        }
        unsafe { wr_rc_inc(value) };
        let obj = Box::new(ResultObj {
            header: header(TypeId::Result),
            is_ok: 0,
            value,
        });
        #[cfg(feature = "metrics")]
        inc_alloc_result();
        Value::from_ptr(Box::into_raw(obj) as *mut ObjHeader)
    }

    pub fn result_is_ok(result: Value) -> Value {
        if !result.is_ptr() {
            return Value::from_bool(false);
        }
        unsafe {
            let header = &*result.as_ptr();
            if header.type_id != TypeId::Result as u32 {
                return Value::from_bool(false);
            }
            let obj = result.as_ptr() as *const ResultObj;
            Value::from_bool((*obj).is_ok != 0)
        }
    }

    pub fn result_unwrap(result: Value) -> Value {
        if !result.is_ptr() {
            return Value::nil();
        }
        unsafe {
            let header = &*result.as_ptr();
            if header.type_id != TypeId::Result as u32 {
                return Value::nil();
            }
            let obj = result.as_ptr() as *const ResultObj;
            let value = (*obj).value;
            wr_rc_inc(value);
            value
        }
    }

    pub fn result_err_unwrap(result: Value) -> Value {
        if !result.is_ptr() {
            return Value::nil();
        }
        unsafe {
            let header = &*result.as_ptr();
            if header.type_id != TypeId::Result as u32 {
                return Value::nil();
            }
            let obj = result.as_ptr() as *const ResultObj;
            if (*obj).is_ok != 0 {
                return Value::nil();
            }
            let value = (*obj).value;
            wr_rc_inc(value);
            value
        }
    }

    pub fn result_parts(result: Value) -> Option<(bool, Value)> {
        if !result.is_ptr() {
            return None;
        }
        unsafe {
            let header = &*result.as_ptr();
            if header.type_id != TypeId::Result as u32 {
                return None;
            }
            let obj = result.as_ptr() as *const ResultObj;
            Some(((*obj).is_ok != 0, (*obj).value))
        }
    }

    pub unsafe fn drop_result(ptr: *mut ObjHeader) {
        let obj = unsafe { Box::from_raw(ptr as *mut ResultObj) };
        unsafe { wr_rc_dec(obj.value) };
    }

    pub fn drop_result_in_arena(ptr: *mut ObjHeader) {
        let obj = ptr as *mut ResultObj;
        unsafe { wr_rc_dec((*obj).value) };
    }
}

pub(crate) mod string {
    use crate::arena;
    #[cfg(feature = "metrics")]
    use crate::metrics::inc_alloc_string;
    use crate::object::ObjHeader;
    use crate::value::{TypeId, Value, header, int_value};
    use crate::{wr_rc_dec, wr_rc_inc};
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    #[repr(C)]
    pub struct StrObj {
        header: ObjHeader,
        bytes: Vec<u8>,
        arena_backed: bool,
    }

    static INTERN: OnceLock<Mutex<HashMap<Vec<u8>, Value>>> = OnceLock::new();

    fn intern_map() -> &'static Mutex<HashMap<Vec<u8>, Value>> {
        INTERN.get_or_init(|| Mutex::new(HashMap::new()))
    }

    pub fn str_from_utf8(ptr: *const u8, len: usize) -> Value {
        if ptr.is_null() && len != 0 {
            return Value::nil();
        }
        let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
        if std::str::from_utf8(bytes).is_err() {
            return Value::nil();
        }
        let s = Box::new(StrObj {
            header: header(TypeId::String),
            bytes: bytes.to_vec(),
            arena_backed: false,
        });
        #[cfg(feature = "metrics")]
        inc_alloc_string();
        Value::from_ptr(Box::into_raw(s) as *mut ObjHeader)
    }

    // Intern a UTF-8 string literal from static bytes without allocating a temporary string first.
    //
    // This is designed for compiler-emitted string literals that are treated as constants:
    // callers typically do not participate in RC for the returned Value.
    pub fn str_intern_utf8(ptr: *const u8, len: usize) -> Value {
        if ptr.is_null() && len != 0 {
            return Value::nil();
        }
        let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
        if std::str::from_utf8(bytes).is_err() {
            return Value::nil();
        }
        let mut map = intern_map().lock().expect("intern map lock");
        if let Some(existing) = map.get(bytes) {
            // Do NOT rc_inc here: string literals are treated as constants by the compiler.
            return *existing;
        }
        let s = Box::new(StrObj {
            header: header(TypeId::String),
            bytes: bytes.to_vec(),
            arena_backed: false,
        });
        #[cfg(feature = "metrics")]
        inc_alloc_string();
        let val = Value::from_ptr(Box::into_raw(s) as *mut ObjHeader);
        // Keep one ref alive for the intern table.
        map.insert(bytes.to_vec(), val);
        unsafe { wr_rc_inc(val) };
        val
    }

    pub fn str_from_bytes(bytes: &[u8]) -> Value {
        let s = Box::new(StrObj {
            header: header(TypeId::String),
            bytes: bytes.to_vec(),
            arena_backed: false,
        });
        #[cfg(feature = "metrics")]
        inc_alloc_string();
        Value::from_ptr(Box::into_raw(s) as *mut ObjHeader)
    }

    pub fn str_intern(val: Value) -> Value {
        if !val.is_ptr() {
            return val;
        }
        unsafe {
            let header = &*val.as_ptr();
            if header.type_id != TypeId::String as u32 {
                return val;
            }
        }
        let bytes = unsafe { &*(val.as_ptr() as *mut StrObj) }.bytes.clone();
        let mut map = intern_map().lock().expect("intern map lock");
        if let Some(existing) = map.get(&bytes) {
            unsafe { wr_rc_dec(val) };
            return *existing;
        }
        map.insert(bytes, val);
        unsafe { wr_rc_inc(val) };
        val
    }

    pub fn str_concat(parts_ptr: *const Value, parts_len: usize) -> Value {
        if parts_ptr.is_null() && parts_len != 0 {
            return Value::nil();
        }
        let parts = unsafe { std::slice::from_raw_parts(parts_ptr, parts_len) };
        let mut total = 0usize;
        for part in parts {
            total = total.saturating_add(value_bytes_len(*part));
        }
        let mut out = Vec::with_capacity(total);
        for part in parts {
            write_value_bytes(*part, &mut out);
        }
        let s = Box::new(StrObj {
            header: header(TypeId::String),
            bytes: out,
            arena_backed: false,
        });
        #[cfg(feature = "metrics")]
        inc_alloc_string();
        Value::from_ptr(Box::into_raw(s) as *mut ObjHeader)
    }

    pub fn str_concat_local(parts_ptr: *const Value, parts_len: usize) -> Value {
        if parts_ptr.is_null() && parts_len != 0 {
            return Value::nil();
        }
        let parts = unsafe { std::slice::from_raw_parts(parts_ptr, parts_len) };
        let mut total = 0usize;
        for part in parts {
            total = total.saturating_add(value_bytes_len(*part));
        }
        if let Some(bytes_ptr) = arena::alloc_bytes_in_current(total, 1) {
            unsafe {
                let mut out = Vec::from_raw_parts(bytes_ptr, 0, total);
                for part in parts {
                    write_value_bytes(*part, &mut out);
                }
                let obj = StrObj {
                    header: header(TypeId::String),
                    bytes: out,
                    arena_backed: true,
                };
                if let Some(ptr) = arena::alloc_in_current(obj) {
                    #[cfg(feature = "metrics")]
                    inc_alloc_string();
                    return Value::from_ptr(ptr as *mut ObjHeader);
                }
            }
        }
        str_concat(parts_ptr, parts_len)
    }

    pub fn drop_string(ptr: *mut ObjHeader) {
        let s = ptr as *mut StrObj;
        unsafe {
            drop(Box::from_raw(s));
        }
    }

    pub fn drop_string_in_arena(ptr: *mut ObjHeader) {
        let s = ptr as *mut StrObj;
        unsafe {
            if !(*s).arena_backed {
                std::ptr::drop_in_place(&mut (*s).bytes);
            }
        }
    }

    pub fn with_string_bytes<F, R>(val: Value, f: F) -> Option<R>
    where
        F: FnOnce(&[u8]) -> R,
    {
        if !val.is_ptr() {
            return None;
        }
        unsafe {
            let header = &*val.as_ptr();
            if header.type_id != TypeId::String as u32 {
                return None;
            }
            let s = &*(val.as_ptr() as *const StrObj);
            Some(f(&s.bytes))
        }
    }

    fn value_bytes_len(val: Value) -> usize {
        if val.is_ptr() {
            unsafe {
                let header = &*val.as_ptr();
                if header.type_id == TypeId::String as u32 {
                    let s = &*(val.as_ptr() as *const StrObj);
                    return s.bytes.len();
                }
            }
        }
        if let Some(i) = int_value(val) {
            return int_len(i);
        }
        if val.is_bool() {
            return if val.as_bool() { 4 } else { 5 };
        }
        if val.is_nil() {
            return 3;
        }
        5
    }

    fn write_value_bytes(val: Value, out: &mut Vec<u8>) {
        if val.is_ptr() {
            unsafe {
                let header = &*val.as_ptr();
                if header.type_id == TypeId::String as u32 {
                    let s = &*(val.as_ptr() as *const StrObj);
                    out.extend_from_slice(&s.bytes);
                    return;
                }
            }
        }
        if let Some(i) = int_value(val) {
            write_int(out, i);
            return;
        }
        if val.is_bool() {
            if val.as_bool() {
                out.extend_from_slice(b"true");
            } else {
                out.extend_from_slice(b"false");
            }
            return;
        }
        if val.is_nil() {
            out.extend_from_slice(b"nothing");
            return;
        }
        out.extend_from_slice(b"<obj>");
    }

    fn int_len(mut val: i64) -> usize {
        if val == 0 {
            return 1;
        }
        if val == i64::MIN {
            return 20;
        }
        let mut len = 0usize;
        if val < 0 {
            len += 1;
            val = -val;
        }
        while val > 0 {
            len += 1;
            val /= 10;
        }
        len
    }

    fn write_int(out: &mut Vec<u8>, val: i64) {
        if val == 0 {
            out.push(b'0');
            return;
        }
        if val == i64::MIN {
            out.extend_from_slice(b"-9223372036854775808");
            return;
        }
        let mut n = val;
        if n < 0 {
            out.push(b'-');
            n = -n;
        }
        let mut buf = [0u8; 20];
        let mut i = 0usize;
        while n > 0 {
            buf[i] = b'0' + (n % 10) as u8;
            n /= 10;
            i += 1;
        }
        for idx in (0..i).rev() {
            out.push(buf[idx]);
        }
    }
}

pub(crate) mod value {
    use crate::bytes::with_bytes;
    use crate::object::ObjHeader;
    use crate::string::with_string_bytes;
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
                    let eq = with_string_bytes(a, |ab| {
                        with_string_bytes(b, |bb| ab == bb).unwrap_or(false)
                    });
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

    pub unsafe fn drop_boxed_int(ptr: *mut ObjHeader) {
        let boxed = ptr as *mut IntBox;
        unsafe {
            drop(Box::from_raw(boxed));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::value::Value;
    use crate::{arena, list, result, string, wr_rc_dec, wr_rc_inc};
    use std::sync::atomic::Ordering;

    fn rc_count(value: Value) -> u32 {
        assert!(value.is_ptr());
        unsafe { (*value.as_ptr()).rc.load(Ordering::Relaxed) }
    }

    #[test]
    fn region_confined_rc_ops_are_elided() {
        let mut arena_mem = arena::Arena::new(1024);
        let _guard = arena::enter(&mut arena_mem as *mut _);

        let local = list::list_new_local(0);
        assert!(arena::is_arena_value(local));
        assert_eq!(rc_count(local), 1);

        unsafe {
            wr_rc_inc(local);
            wr_rc_dec(local);
        }

        // The global metrics counters are process-wide and can be advanced by
        // other concurrently running tests, so this assertion focuses on
        // observable local behavior.
        assert_eq!(rc_count(local), 1);

        arena_mem.reset();
    }

    #[test]
    fn region_bulk_reclaim_drops_external_refs() {
        let heap = string::str_from_bytes(b"owned-by-heap");
        assert_eq!(rc_count(heap), 1);

        let mut arena_mem = arena::Arena::new(1024);
        {
            let _guard = arena::enter(&mut arena_mem as *mut _);
            let local = list::list_new_local(0);
            list::list_push(local, heap);
            assert_eq!(rc_count(heap), 2);
            arena_mem.reset();
        }

        assert_eq!(rc_count(heap), 1);
        unsafe { wr_rc_dec(heap) };
    }

    #[test]
    fn result_rejects_region_confined_escape() {
        let mut arena_mem = arena::Arena::new(1024);
        let _guard = arena::enter(&mut arena_mem as *mut _);

        let local = string::str_concat_local([Value::from_int(7)].as_ptr(), 1);
        assert!(arena::is_arena_value(local));
        assert!(arena::reject_arena_escape(local, "test").is_none());

        let wrapped = result::result_ok(local);
        assert!(wrapped.is_nil());

        arena_mem.reset();
    }
}
