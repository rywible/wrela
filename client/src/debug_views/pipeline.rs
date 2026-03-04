#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code, unused_imports))]

#[cfg(target_arch = "wasm32")]
use std::borrow::Cow;

#[cfg(target_arch = "wasm32")]
use super::shaders;
#[cfg(target_arch = "wasm32")]
use super::DebugViewMode;

/// Create a fullscreen overlay pipeline for a debug view shader.
#[cfg(target_arch = "wasm32")]
pub fn create_debug_view_pipeline(
    device: &wgpu::Device,
    label: &str,
    shader_source: &str,
    bind_group_layout: &wgpu::BindGroupLayout,
    target_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader_source)),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[bind_group_layout],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs"),
            targets: &[Some(wgpu::ColorTargetState {
                format: target_format,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::SrcAlpha,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent::OVER,
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

/// Get the shader source for a debug view mode.
#[cfg(target_arch = "wasm32")]
pub fn shader_for_mode(mode: DebugViewMode) -> &'static str {
    match mode {
        DebugViewMode::StepCount => shaders::STEP_COUNT_SHADER,
        DebugViewMode::BoundMagnitude => shaders::BOUND_MAGNITUDE_SHADER,
        DebugViewMode::BProvenance => shaders::B_PROVENANCE_SHADER,
        _ => shaders::PLACEHOLDER_SHADER,
    }
}
