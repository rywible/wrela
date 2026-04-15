use crate::gpu_runtime::layout::{
    GPU_RUNTIME_BIND_GROUP_COUNT, GpuLayoutIdentity, signature_for_hashable,
};
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GpuBindGroupRole {
    SceneStatic,
    Frame,
    Pass,
    Scratch,
}

impl GpuBindGroupRole {
    pub const fn index(self) -> u32 {
        match self {
            Self::SceneStatic => 0,
            Self::Frame => 1,
            Self::Pass => 2,
            Self::Scratch => 3,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::SceneStatic => "scene_static",
            Self::Frame => "frame",
            Self::Pass => "pass",
            Self::Scratch => "scratch",
        }
    }

    pub const fn all() -> [Self; GPU_RUNTIME_BIND_GROUP_COUNT as usize] {
        [Self::SceneStatic, Self::Frame, Self::Pass, Self::Scratch]
    }
}

pub fn bind_group_layout_signature(descriptor: &wgpu::BindGroupLayoutDescriptor<'_>) -> u64 {
    let mut entries = descriptor.entries.to_vec();
    entries.sort_by_key(|entry| entry.binding);
    signature_for_hashable(&entries)
}

pub fn bind_group_layout_signature_for_role(
    role: GpuBindGroupRole,
    descriptor: &wgpu::BindGroupLayoutDescriptor<'_>,
) -> u64 {
    let mut entries = descriptor.entries.to_vec();
    entries.sort_by_key(|entry| entry.binding);
    signature_for_hashable(&(role.index(), entries))
}

pub fn pipeline_layout_signature(bind_group_layout_signatures: &[u64], immediate_size: u32) -> u64 {
    signature_for_hashable(&(
        GPU_RUNTIME_BIND_GROUP_COUNT,
        bind_group_layout_signatures,
        immediate_size,
    ))
}

pub fn pipeline_layout_identity(
    bind_group_layout_signatures: &[u64],
    immediate_size: u32,
    feature_mask: u64,
) -> GpuLayoutIdentity {
    GpuLayoutIdentity::new(
        pipeline_layout_signature(bind_group_layout_signatures, immediate_size),
        feature_mask,
    )
}

pub fn texture_view_binding_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
    sample_type: wgpu::TextureSampleType,
    view_dimension: wgpu::TextureViewDimension,
    multisampled: bool,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Texture {
            sample_type,
            view_dimension,
            multisampled,
        },
        count: None,
    }
}

pub fn storage_buffer_binding_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
    read_only: bool,
    has_dynamic_offset: bool,
    min_binding_size: Option<NonZeroU32>,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset,
            min_binding_size: min_binding_size
                .and_then(|value| wgpu::BufferSize::new(u64::from(value.get()))),
        },
        count: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(
        entries: Vec<wgpu::BindGroupLayoutEntry>,
    ) -> wgpu::BindGroupLayoutDescriptor<'static> {
        wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: Box::leak(entries.into_boxed_slice()),
        }
    }

    #[test]
    fn bind_group_layout_signature_is_order_independent() {
        let left = descriptor(vec![
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ]);
        let right = descriptor(vec![
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ]);
        assert_eq!(
            bind_group_layout_signature(&left),
            bind_group_layout_signature(&right)
        );
    }

    #[test]
    fn pipeline_layout_identity_reflects_feature_mask() {
        let signatures = [11, 29, 41, 53];
        let left = pipeline_layout_identity(&signatures, 0, 7);
        let right = pipeline_layout_identity(&signatures, 0, 9);
        assert_ne!(left, right);
        assert_eq!(left.layout_signature, right.layout_signature);
    }

    #[test]
    fn bind_group_roles_have_stable_indexes() {
        assert_eq!(GpuBindGroupRole::SceneStatic.index(), 0);
        assert_eq!(GpuBindGroupRole::Frame.index(), 1);
        assert_eq!(GpuBindGroupRole::Pass.index(), 2);
        assert_eq!(GpuBindGroupRole::Scratch.index(), 3);
    }
}
