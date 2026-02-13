use crate::db::time::hlc::HlTimestamp;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UncertaintyWindow {
    pub lower_bound: u64,
    pub upper_bound: u64,
}

#[derive(Debug)]
pub struct UncertaintyTracker {
    max_clock_skew_ms: u64,
    observed_remote_max_physical_ms: Mutex<u64>,
}

impl UncertaintyTracker {
    pub fn new(max_clock_skew_ms: u64) -> Self {
        Self {
            max_clock_skew_ms,
            observed_remote_max_physical_ms: Mutex::new(0),
        }
    }

    pub fn observe_remote_packed(&self, packed: u64) {
        let remote = HlTimestamp::unpack(packed);
        let mut observed = self
            .observed_remote_max_physical_ms
            .lock()
            .expect("uncertainty lock");
        *observed = (*observed).max(remote.physical_ms);
    }

    pub fn window_for_read_packed(&self, read_packed: u64) -> UncertaintyWindow {
        let read = HlTimestamp::unpack(read_packed);
        let observed_remote_max = *self
            .observed_remote_max_physical_ms
            .lock()
            .expect("uncertainty lock");

        let upper_physical = read
            .physical_ms
            .max(observed_remote_max)
            .saturating_add(self.max_clock_skew_ms);

        UncertaintyWindow {
            lower_bound: read.pack(),
            upper_bound: HlTimestamp {
                physical_ms: upper_physical,
                logical: u16::MAX,
            }
            .pack(),
        }
    }

    pub fn max_clock_skew_ms(&self) -> u64 {
        self.max_clock_skew_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_includes_configured_skew() {
        let tracker = UncertaintyTracker::new(25);
        let read = HlTimestamp {
            physical_ms: 1000,
            logical: 1,
        }
        .pack();
        let window = tracker.window_for_read_packed(read);
        assert_eq!(window.lower_bound, read);
        assert!(window.upper_bound > window.lower_bound);
    }

    #[test]
    fn remote_observation_extends_upper_bound() {
        let tracker = UncertaintyTracker::new(10);
        let read = HlTimestamp {
            physical_ms: 100,
            logical: 1,
        }
        .pack();
        let remote = HlTimestamp {
            physical_ms: 150,
            logical: 7,
        }
        .pack();
        tracker.observe_remote_packed(remote);
        let window = tracker.window_for_read_packed(read);
        let remote_with_skew = HlTimestamp {
            physical_ms: 160,
            logical: u16::MAX,
        }
        .pack();
        assert!(window.upper_bound >= remote_with_skew);
    }
}
