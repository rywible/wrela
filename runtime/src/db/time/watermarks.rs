use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug)]
pub struct SafeReadWatermarks {
    by_node: Mutex<HashMap<u64, u64>>,
}

impl SafeReadWatermarks {
    pub fn new() -> Self {
        Self {
            by_node: Mutex::new(HashMap::new()),
        }
    }

    pub fn observe(&self, node_id: u64, safe_ts: u64) {
        let mut by_node = self.by_node.lock().expect("watermark lock");
        let entry = by_node.entry(node_id).or_insert(0);
        *entry = (*entry).max(safe_ts);
    }

    pub fn node_safe_read(&self, node_id: u64) -> Option<u64> {
        self.by_node
            .lock()
            .expect("watermark lock")
            .get(&node_id)
            .copied()
    }

    pub fn global_safe_read(&self) -> Option<u64> {
        self.by_node
            .lock()
            .expect("watermark lock")
            .values()
            .copied()
            .min()
    }
}

impl Default for SafeReadWatermarks {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_node_and_global_watermarks_are_monotonic() {
        let wm = SafeReadWatermarks::new();
        wm.observe(1, 100);
        wm.observe(2, 90);
        wm.observe(1, 95);

        assert_eq!(wm.node_safe_read(1), Some(100));
        assert_eq!(wm.node_safe_read(2), Some(90));
        assert_eq!(wm.global_safe_read(), Some(90));
    }
}
