/// Blue noise sampling service.
pub struct BlueNoiseService {
    data: Vec<u8>,
    width: usize,
    height: usize,
    depth: usize,
    deterministic: bool,
}

impl BlueNoiseService {
    /// Create a new service with the given blue noise data and dimensions.
    pub fn new(data: Vec<u8>, width: usize, height: usize, depth: usize, deterministic: bool) -> Self {
        assert_eq!(
            data.len(),
            width * height * depth,
            "Blue noise data has wrong size"
        );
        Self {
            data,
            width,
            height,
            depth,
            deterministic,
        }
    }

    /// Sample the blue noise texture.
    /// Returns a value in [0.0, 1.0).
    pub fn sample(&self, x: u32, y: u32, frame: u32, system_id: u32) -> f32 {
        let px = (x as usize) % self.width;
        let py = (y as usize) % self.height;

        // In deterministic mode, freeze at frame 0
        let pz = if self.deterministic {
            0
        } else {
            // Offset by system_id for per-system decorrelation
            ((frame as usize) + (system_id as usize) * 7) % self.depth
        };

        let idx = pz * self.width * self.height + py * self.width + px;
        self.data[idx] as f32 / 256.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blue_noise::generate::generate_blue_noise_custom;

    const TW: usize = 32;
    const TH: usize = 32;
    const TD: usize = 4;

    fn test_service(deterministic: bool) -> BlueNoiseService {
        let data = generate_blue_noise_custom(TW, TH, TD, 42);
        BlueNoiseService::new(data, TW, TH, TD, deterministic)
    }

    #[test]
    fn deterministic_mode_frozen() {
        let service = test_service(true);

        let a = service.sample(10, 20, 0, 1);
        let b = service.sample(10, 20, 3, 1);
        assert_eq!(a, b); // Same in deterministic mode regardless of frame
    }

    #[test]
    fn different_systems_differ() {
        let service = test_service(false);

        let a = service.sample(10, 20, 1, 1);
        let b = service.sample(10, 20, 1, 2);
        assert_ne!(a, b);
    }

    #[test]
    fn sample_range() {
        let service = test_service(false);

        for x in 0..TW as u32 {
            for y in 0..TH as u32 {
                let v = service.sample(x, y, 0, 0);
                assert!(v >= 0.0 && v < 1.0, "Out of range: {}", v);
            }
        }
    }
}
