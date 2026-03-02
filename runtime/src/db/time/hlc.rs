use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const LOGICAL_BITS: u32 = 16;
const LOGICAL_MASK: u64 = (1u64 << LOGICAL_BITS) - 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct HlTimestamp {
    pub physical_ms: u64,
    pub logical: u16,
}

impl HlTimestamp {
    pub fn pack(self) -> u64 {
        (self.physical_ms << LOGICAL_BITS) | (self.logical as u64)
    }

    pub fn unpack(packed: u64) -> Self {
        Self {
            physical_ms: packed >> LOGICAL_BITS,
            logical: (packed & LOGICAL_MASK) as u16,
        }
    }
}

#[derive(Debug)]
pub struct HybridLogicalClock {
    state: Mutex<HlTimestamp>,
}

impl HybridLogicalClock {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(HlTimestamp {
                physical_ms: wall_clock_ms(),
                logical: 0,
            }),
        }
    }

    pub fn tick(&self) -> HlTimestamp {
        let mut state = self.state.lock().expect("HLC lock");
        let wall = wall_clock_ms();
        if wall > state.physical_ms {
            state.physical_ms = wall;
            state.logical = 0;
            return *state;
        }
        if state.logical == u16::MAX {
            state.physical_ms = state.physical_ms.saturating_add(1);
            state.logical = 0;
            return *state;
        }
        state.logical = state.logical.saturating_add(1);
        *state
    }

    /// Returns `count` monotonically increasing packed timestamps with a single wall-clock read.
    /// Use in batch prepare to avoid one SystemTime::now() per op.
    pub fn tick_batch(&self, count: usize) -> Vec<u64> {
        if count == 0 {
            return Vec::new();
        }
        let wall = wall_clock_ms();
        let mut state = self.state.lock().expect("HLC lock");
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            if wall > state.physical_ms {
                state.physical_ms = wall;
                state.logical = 0;
            } else if state.logical == u16::MAX {
                state.physical_ms = state.physical_ms.saturating_add(1);
                state.logical = 0;
            } else {
                state.logical = state.logical.saturating_add(1);
            }
            out.push(state.pack());
        }
        out
    }

    pub fn observe_packed(&self, packed: u64) -> HlTimestamp {
        self.observe(HlTimestamp::unpack(packed))
    }

    pub fn observe(&self, remote: HlTimestamp) -> HlTimestamp {
        let mut state = self.state.lock().expect("HLC lock");
        let wall = wall_clock_ms();
        let max_physical = wall.max(state.physical_ms).max(remote.physical_ms);
        if max_physical == state.physical_ms && max_physical == remote.physical_ms {
            if state.logical == u16::MAX {
                state.physical_ms = state.physical_ms.saturating_add(1);
                state.logical = 0;
            } else {
                state.logical = state.logical.max(remote.logical).saturating_add(1);
            }
        } else if max_physical == state.physical_ms {
            if state.logical == u16::MAX {
                state.physical_ms = state.physical_ms.saturating_add(1);
                state.logical = 0;
            } else {
                state.logical = state.logical.saturating_add(1);
            }
        } else if max_physical == remote.physical_ms {
            if remote.logical == u16::MAX {
                state.physical_ms = remote.physical_ms.saturating_add(1);
                state.logical = 0;
            } else {
                state.physical_ms = remote.physical_ms;
                state.logical = remote.logical.saturating_add(1);
            }
        } else {
            state.physical_ms = wall;
            state.logical = 0;
        }
        *state
    }

    pub fn peek(&self) -> HlTimestamp {
        *self.state.lock().expect("HLC lock")
    }
}

impl Default for HybridLogicalClock {
    fn default() -> Self {
        Self::new()
    }
}

fn wall_clock_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_is_monotonic() {
        let clock = HybridLogicalClock::new();
        let t1 = clock.tick();
        let t2 = clock.tick();
        assert!(t2.pack() > t1.pack());
    }

    #[test]
    fn observe_remote_advances_local() {
        let clock = HybridLogicalClock::new();
        let local = clock.tick();
        let remote = HlTimestamp {
            physical_ms: local.physical_ms.saturating_add(10),
            logical: 42,
        };
        let merged = clock.observe(remote);
        assert!(merged.pack() > local.pack());
        assert!(merged.pack() > remote.pack());
    }
}
