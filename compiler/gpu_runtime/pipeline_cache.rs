use crate::gpu_runtime::bindings::pipeline_layout_identity;
use crate::gpu_runtime::layout::{GpuLayoutIdentity, signature_for_hashable};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, hash::Hash, sync::Arc};

#[derive(Debug)]
pub struct GpuResourceCache<K, V> {
    entries: HashMap<K, Arc<V>>,
}

impl<K: Eq + Hash, V> GpuResourceCache<K, V> {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn get(&self, key: &K) -> Option<Arc<V>> {
        self.entries.get(key).cloned()
    }

    pub fn insert(&mut self, key: K, value: V) -> Arc<V> {
        let value = Arc::new(value);
        self.entries.insert(key, value.clone());
        value
    }

    pub fn get_or_insert_with<E, F>(&mut self, key: K, build: F) -> Result<Arc<V>, E>
    where
        F: FnOnce(&K) -> Result<V, E>,
    {
        if let Some(existing) = self.entries.get(&key) {
            return Ok(existing.clone());
        }
        let built = Arc::new(build(&key)?);
        self.entries.insert(key, built.clone());
        Ok(built)
    }
}

impl<K: Eq + Hash, V> Default for GpuResourceCache<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PipelineLayoutKey {
    pub layout: GpuLayoutIdentity,
    pub bind_group_count: u32,
    pub immediate_size: u32,
}

impl PipelineLayoutKey {
    pub const fn new(
        layout: GpuLayoutIdentity,
        bind_group_count: u32,
        immediate_size: u32,
    ) -> Self {
        Self {
            layout,
            bind_group_count,
            immediate_size,
        }
    }

    pub fn from_bind_group_layout_signatures(
        bind_group_layout_signatures: &[u64],
        immediate_size: u32,
        feature_mask: u64,
    ) -> Self {
        Self::new(
            pipeline_layout_identity(bind_group_layout_signatures, immediate_size, feature_mask),
            bind_group_layout_signatures.len() as u32,
            immediate_size,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ComputePipelineKey {
    pub layout: PipelineLayoutKey,
    pub shader_signature: u64,
    pub entry_point: String,
    pub workgroup_size: u32,
}

impl ComputePipelineKey {
    pub fn new(
        layout: PipelineLayoutKey,
        shader_signature: u64,
        entry_point: impl Into<String>,
        workgroup_size: u32,
    ) -> Self {
        Self {
            layout,
            shader_signature,
            entry_point: entry_point.into(),
            workgroup_size,
        }
    }

    pub fn from_shader_source(
        layout: PipelineLayoutKey,
        shader_source: &str,
        entry_point: impl Into<String>,
        workgroup_size: u32,
    ) -> Self {
        Self::new(
            layout,
            shader_signature(shader_source),
            entry_point,
            workgroup_size,
        )
    }
}

pub type BindGroupLayoutCache = GpuResourceCache<u64, wgpu::BindGroupLayout>;
pub type PipelineLayoutCache = GpuResourceCache<PipelineLayoutKey, wgpu::PipelineLayout>;
pub type ComputePipelineCache = GpuResourceCache<ComputePipelineKey, wgpu::ComputePipeline>;

pub fn shader_signature(shader_source: &str) -> u64 {
    signature_for_hashable(&shader_source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn resource_cache_reuses_existing_entries() {
        let mut cache = GpuResourceCache::<u32, String>::new();
        let build_count = Cell::new(0);

        let left = cache
            .get_or_insert_with(11, |_| {
                build_count.set(build_count.get() + 1);
                Ok::<_, ()>("scene".to_string())
            })
            .expect("first insert");
        let right = cache
            .get_or_insert_with(11, |_| {
                build_count.set(build_count.get() + 1);
                Ok::<_, ()>("different".to_string())
            })
            .expect("cache hit");

        assert_eq!(build_count.get(), 1);
        assert!(Arc::ptr_eq(&left, &right));
        assert_eq!(&*left, "scene");
    }

    #[test]
    fn pipeline_layout_key_tracks_layout_and_immediate_size() {
        let layout = GpuLayoutIdentity::new(37, 11);
        let left = PipelineLayoutKey::new(layout, 4, 0);
        let right = PipelineLayoutKey::new(layout, 4, 8);
        assert_ne!(left, right);
        assert_eq!(left.layout, right.layout);
    }

    #[test]
    fn compute_pipeline_key_uses_shader_signature() {
        let layout = PipelineLayoutKey::new(GpuLayoutIdentity::new(4, 9), 4, 0);
        let left = ComputePipelineKey::from_shader_source(
            layout.clone(),
            "@compute fn main() {}",
            "main",
            64,
        );
        let right =
            ComputePipelineKey::from_shader_source(layout, "@compute fn main() {}", "main", 64);
        assert_eq!(left, right);
    }

    #[test]
    fn compute_pipeline_key_tracks_workgroup_size() {
        let layout = PipelineLayoutKey::new(GpuLayoutIdentity::new(4, 9), 4, 0);
        let left = ComputePipelineKey::from_shader_source(
            layout.clone(),
            "@compute fn main() {}",
            "main",
            32,
        );
        let right =
            ComputePipelineKey::from_shader_source(layout, "@compute fn main() {}", "main", 64);
        assert_ne!(left, right);
    }
}
