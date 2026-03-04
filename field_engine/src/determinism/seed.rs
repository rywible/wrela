/// Deterministic seed infrastructure.

/// World-level determinism state.
#[derive(Debug, Clone)]
pub struct DeterminismState {
    pub world_seed: u64,
    pub sim_tick: u64,
    pub render_frame: u64,
}

impl DeterminismState {
    pub fn new(world_seed: u64) -> Self {
        Self {
            world_seed,
            sim_tick: 0,
            render_frame: 0,
        }
    }

    /// Deterministic sub-seed for a system at the current simulation tick.
    pub fn system_seed(&self, system_id: u32) -> u64 {
        hash_combine(&[self.world_seed, self.sim_tick, system_id as u64])
    }

    /// Deterministic sub-seed for a pixel in a rendering system.
    pub fn pixel_seed(&self, system_id: u32, x: u32, y: u32) -> u64 {
        hash_combine(&[
            self.world_seed,
            self.render_frame,
            system_id as u64,
            x as u64,
            y as u64,
        ])
    }
}

/// Simple deterministic hash combining (FNV-1a style, good enough for seeds).
fn hash_combine(values: &[u64]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325; // FNV offset basis
    for &v in values {
        let bytes = v.to_le_bytes();
        for &b in &bytes {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x100000001b3); // FNV prime
        }
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_output() {
        let state = DeterminismState::new(42);
        let a = state.system_seed(1);
        let b = state.system_seed(1);
        assert_eq!(a, b);
    }

    #[test]
    fn different_ticks_different_seeds() {
        let mut state = DeterminismState::new(42);
        let a = state.system_seed(1);
        state.sim_tick = 1;
        let b = state.system_seed(1);
        assert_ne!(a, b);
    }

    #[test]
    fn different_systems_different_seeds() {
        let state = DeterminismState::new(42);
        let a = state.system_seed(1);
        let b = state.system_seed(2);
        assert_ne!(a, b);
    }

    #[test]
    fn pixel_seed_deterministic() {
        let state = DeterminismState::new(42);
        let a = state.pixel_seed(1, 100, 200);
        let b = state.pixel_seed(1, 100, 200);
        assert_eq!(a, b);
    }

    #[test]
    fn different_pixels_different_seeds() {
        let state = DeterminismState::new(42);
        let a = state.pixel_seed(1, 100, 200);
        let b = state.pixel_seed(1, 101, 200);
        assert_ne!(a, b);
    }
}
