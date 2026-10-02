//! The CPU state hash: 64-bit FNV-1a over every byte a program submits, in order (AC7).
//!
//! FNV-1a because it's a few lines in any language, so the browser and native hosts can't
//! disagree about it; it's a fingerprint for comparing two runs, not a defence against anyone.

use std::fmt;

/// FNV-1a's 64-bit offset basis: the hash of no bytes.
pub const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;

/// FNV-1a's 64-bit prime.
pub const PRIME: u64 = 0x0000_0100_0000_01b3;

/// A running FNV-1a hash. Hashing bytes in pieces gives the same result as hashing them at once.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct StateHash(u64);

impl Default for StateHash {
    fn default() -> Self {
        StateHash::new()
    }
}

impl StateHash {
    pub const fn new() -> Self {
        StateHash(OFFSET_BASIS)
    }

    pub fn update(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(PRIME);
        }
    }

    pub const fn value(self) -> u64 {
        self.0
    }

    /// The hash as hosts report it: 16 lowercase hex digits.
    pub fn hex(self) -> String {
        format!("{:016x}", self.0)
    }
}

impl fmt::Display for StateHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(bytes: &[u8]) -> StateHash {
        let mut h = StateHash::new();
        h.update(bytes);
        h
    }

    /// The published FNV-1a 64 test vectors (and one for a version-0 header).
    #[test]
    fn known_vectors() {
        assert_eq!(hash(b"").value(), 0xcbf2_9ce4_8422_2325);
        assert_eq!(hash(b"a").value(), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(hash(b"foobar").value(), 0x8594_4171_f739_67e8);
        assert_eq!(hash(b"WR\0\0").hex(), "f9ff4f026da78e4c");
    }

    #[test]
    fn pieces_hash_like_the_whole() {
        let bytes: Vec<u8> = (0..=255).collect();
        let whole = hash(&bytes);
        for split in [0, 1, 7, 128, 255, 256] {
            let mut h = StateHash::new();
            h.update(&bytes[..split]);
            h.update(&bytes[split..]);
            assert_eq!(h, whole, "split at {split}");
        }
    }

    #[test]
    fn hex_keeps_leading_zeros() {
        assert_eq!(StateHash(0xab).hex(), "00000000000000ab");
        assert_eq!(StateHash(0xab).to_string(), "00000000000000ab");
        assert_eq!(StateHash::default(), StateHash::new());
    }
}
