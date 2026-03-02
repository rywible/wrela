use std::collections::{HashSet, VecDeque};
use std::sync::Mutex;

#[derive(Debug, Default)]
struct NegativeState {
    order: VecDeque<Vec<u8>>,
    keys: HashSet<Vec<u8>>,
}

#[derive(Debug)]
pub struct NegativeBloom {
    capacity: usize,
    state: Mutex<NegativeState>,
}

impl NegativeBloom {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            state: Mutex::new(NegativeState::default()),
        }
    }

    pub fn probably_absent(&self, key: &[u8]) -> bool {
        self.state
            .lock()
            .expect("negative bloom lock")
            .keys
            .contains(key)
    }

    pub fn record_absent(&self, key: &[u8]) {
        let mut state = self.state.lock().expect("negative bloom lock");
        if state.keys.contains(key) {
            return;
        }
        if state.order.len() >= self.capacity
            && let Some(oldest) = state.order.pop_front()
        {
            state.keys.remove(&oldest);
        }
        let key_vec = key.to_vec();
        state.order.push_back(key_vec.clone());
        state.keys.insert(key_vec);
    }

    pub fn record_present(&self, key: &[u8]) {
        let mut state = self.state.lock().expect("negative bloom lock");
        if state.keys.remove(key) {
            state.order.retain(|candidate| candidate.as_slice() != key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_repeated_misses_and_is_invalidated_by_present() {
        let bloom = NegativeBloom::new(8);
        assert!(!bloom.probably_absent(b"k1"));
        bloom.record_absent(b"k1");
        assert!(bloom.probably_absent(b"k1"));
        bloom.record_present(b"k1");
        assert!(!bloom.probably_absent(b"k1"));
    }
}
