/// Replay harness types and region hash protocol.

/// A snapshot of inputs for replay.
#[derive(Debug, Clone)]
pub struct ReplayFrame {
    pub tick: u64,
    pub world_seed: u64,
    pub input_state: Vec<u8>,
}

/// Region hash for divergence detection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionHash {
    pub region_id: u64,
    pub tick: u64,
    pub hash: u64,
}

impl RegionHash {
    pub fn compute(region_id: u64, tick: u64, state_bytes: &[u8]) -> Self {
        let mut hash: u64 = 0xcbf29ce484222325;
        // Mix region_id and tick into hash
        for &b in &region_id.to_le_bytes() {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        for &b in &tick.to_le_bytes() {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        // Hash state
        for &b in state_bytes {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        Self {
            region_id,
            tick,
            hash,
        }
    }
}

/// Replay recording session.
#[derive(Debug, Clone)]
pub struct ReplayRecording {
    pub frames: Vec<ReplayFrame>,
    pub region_hashes: Vec<RegionHash>,
}

impl ReplayRecording {
    pub fn new() -> Self {
        Self {
            frames: Vec::new(),
            region_hashes: Vec::new(),
        }
    }

    pub fn record_frame(&mut self, frame: ReplayFrame) {
        self.frames.push(frame);
    }

    pub fn record_region_hash(&mut self, hash: RegionHash) {
        self.region_hashes.push(hash);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_hash_deterministic() {
        let a = RegionHash::compute(1, 100, &[1, 2, 3, 4]);
        let b = RegionHash::compute(1, 100, &[1, 2, 3, 4]);
        assert_eq!(a, b);
    }

    #[test]
    fn region_hash_differs_on_state() {
        let a = RegionHash::compute(1, 100, &[1, 2, 3, 4]);
        let b = RegionHash::compute(1, 100, &[1, 2, 3, 5]);
        assert_ne!(a.hash, b.hash);
    }

    #[test]
    fn region_hash_differs_on_tick() {
        let a = RegionHash::compute(1, 100, &[1, 2, 3, 4]);
        let b = RegionHash::compute(1, 101, &[1, 2, 3, 4]);
        assert_ne!(a.hash, b.hash);
    }

    #[test]
    fn replay_roundtrip() {
        let mut recording = ReplayRecording::new();
        let frame = ReplayFrame {
            tick: 0,
            world_seed: 42,
            input_state: vec![0, 1, 0, 0],
        };
        recording.record_frame(frame.clone());
        assert_eq!(recording.frames.len(), 1);
        assert_eq!(recording.frames[0].tick, 0);
        assert_eq!(recording.frames[0].world_seed, 42);
    }
}
