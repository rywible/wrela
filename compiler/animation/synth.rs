use sha2::{Digest, Sha256};

pub fn deterministic_seed(namespace: &str, labels: &[&str]) -> u64 {
    let mut hasher = Sha256::new();
    hasher.update(namespace.as_bytes());
    hasher.update([0xFF]);
    for label in labels {
        hasher.update(label.as_bytes());
        hasher.update([0x00]);
    }
    let digest = hasher.finalize();
    let seed_bytes: [u8; 8] = digest[0..8]
        .try_into()
        .expect("seed digest prefix should be exactly 8 bytes");
    u64::from_le_bytes(seed_bytes)
}

#[cfg(test)]
mod tests {
    use super::deterministic_seed;

    #[test]
    fn seed_stability() {
        let a = deterministic_seed("traveller", &["light", "combo", "v1"]);
        let b = deterministic_seed("traveller", &["light", "combo", "v1"]);
        let c = deterministic_seed("traveller", &["light", "combo", "v2"]);

        assert_eq!(a, b, "same namespace + labels must produce stable seed");
        assert_ne!(a, c, "seed must change when labels change");
    }
}
