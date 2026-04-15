use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::hash::{Hash, Hasher};

pub const GPU_RUNTIME_SCHEMA_VERSION: u32 = 1;
pub const GPU_RUNTIME_BIND_GROUP_COUNT: u32 = 4;
pub const GPU_RUNTIME_SCENE_BIND_GROUP_INDEX: u32 = 0;
pub const GPU_RUNTIME_FRAME_BIND_GROUP_INDEX: u32 = 1;
pub const GPU_RUNTIME_PASS_BIND_GROUP_INDEX: u32 = 2;
pub const GPU_RUNTIME_SCRATCH_BIND_GROUP_INDEX: u32 = 3;

pub const GPU_RUNTIME_FEATURE_TIMESTAMP_QUERY: u64 = 1 << 0;
pub const GPU_RUNTIME_FEATURE_TIMESTAMP_QUERY_INSIDE_PASSES: u64 = 1 << 1;
pub const GPU_RUNTIME_FEATURE_SHADER_F16: u64 = 1 << 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct GpuLayoutIdentity {
    pub layout_signature: u64,
    pub feature_mask: u64,
}

impl GpuLayoutIdentity {
    pub const fn new(layout_signature: u64, feature_mask: u64) -> Self {
        Self {
            layout_signature,
            feature_mask,
        }
    }

    pub const fn with_feature_mask(self, feature_mask: u64) -> Self {
        Self {
            feature_mask,
            ..self
        }
    }
}

#[derive(Clone)]
pub(crate) struct SignatureHasher(Sha256);

impl Default for SignatureHasher {
    fn default() -> Self {
        Self(Sha256::new())
    }
}

impl Hasher for SignatureHasher {
    fn finish(&self) -> u64 {
        let digest = self.0.clone().finalize();
        u64::from_le_bytes(digest[..8].try_into().expect("sha256 digest prefix"))
    }

    fn write(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }
}

pub(crate) fn signature_for_hashable<T: Hash>(value: &T) -> u64 {
    let mut hasher = SignatureHasher::default();
    value.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_for_hashable_is_deterministic() {
        let left = signature_for_hashable(&vec![1u32, 7, 19]);
        let right = signature_for_hashable(&vec![1u32, 7, 19]);
        assert_eq!(left, right);
    }

    #[test]
    fn gpu_layout_identity_keeps_fields() {
        let identity = GpuLayoutIdentity::new(17, 23);
        assert_eq!(identity.layout_signature, 17);
        assert_eq!(identity.feature_mask, 23);
        assert_eq!(identity.with_feature_mask(41).feature_mask, 41);
    }
}
