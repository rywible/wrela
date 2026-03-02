use std::cell::RefCell;

thread_local! {
    static DB_ABI_SCRATCH: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

pub fn with_scratch<F, R>(min_capacity: usize, f: F) -> R
where
    F: FnOnce(&mut Vec<u8>) -> R,
{
    DB_ABI_SCRATCH.with(|slot| {
        let mut scratch = slot.borrow_mut();
        let current_capacity = scratch.capacity();
        if current_capacity < min_capacity {
            scratch.reserve(min_capacity - current_capacity);
        }
        scratch.clear();
        f(&mut scratch)
    })
}
