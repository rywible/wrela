//! Owns WGSL-side presentation ABI/config helpers.
//! Does not own pass execution or framegraph scheduling.
//!
//! Key invariants:
//! - ABI/config structs emitted here must stay aligned with portable/kernel
//!   layouts shared with CPU execution.
//! - helper defaults here are part of the runtime contract, not placeholder
//!   values to be fixed up later.
//!
//! Primary entrypoints:
//! - WGSL ABI/config helpers in this module
//!
//! Failure modes / common pitfalls:
//! - drifting struct shape or default field meaning here breaks shaders without
//!   obvious type-level failures in Rust.

use super::*;

pub(super) fn presentation_dispatch_config(item_count: u32) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("WgslDispatchConfig"),
        fields: vec![
            (SmolStr::new("capture_kind"), KernelValue::U32(0)),
            (SmolStr::new("capture_index"), KernelValue::U32(0)),
            (SmolStr::new("item_count"), KernelValue::U32(item_count)),
            (SmolStr::new("shape_count"), KernelValue::U32(0)),
            (SmolStr::new("accel_root_index"), KernelValue::U32(0)),
            (SmolStr::new("accel_node_count"), KernelValue::U32(0)),
            (SmolStr::new("cache_brick_count"), KernelValue::U32(0)),
            (SmolStr::new("material_enabled"), KernelValue::Bool(false)),
            (SmolStr::new("radiance_enabled"), KernelValue::Bool(false)),
            (SmolStr::new("media_enabled"), KernelValue::Bool(false)),
            (
                SmolStr::new("candidate_spans_enabled"),
                KernelValue::Bool(false),
            ),
        ],
    })
}

#[cfg(test)]
pub(super) fn shade_primary_input_abi() -> PortableAbiType {
    PortableAbiType::Struct {
        name: SmolStr::new("ShadePrimaryInput"),
        class_id: 0,
        fields: vec![
            PortableStructField {
                name: SmolStr::new("hit"),
                ty: portable_builtin_record_abi("Hit3").expect("Hit3 abi"),
            },
            PortableStructField {
                name: SmolStr::new("surface"),
                ty: portable_builtin_record_abi("Surface").expect("Surface abi"),
            },
            PortableStructField {
                name: SmolStr::new("radiance"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("medium"),
                ty: portable_builtin_record_abi("Medium").expect("Medium abi"),
            },
            PortableStructField {
                name: SmolStr::new("ray_direction"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("camera_position"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("lighting"),
                ty: lighting_inputs_abi(),
            },
        ],
    }
}

pub(super) fn lighting_inputs_abi() -> PortableAbiType {
    PortableAbiType::Struct {
        name: SmolStr::new("PresentationLightingInputs"),
        class_id: 0,
        fields: vec![
            PortableStructField {
                name: SmolStr::new("key_light"),
                ty: portable_builtin_record_abi("Light").expect("Light abi"),
            },
            PortableStructField {
                name: SmolStr::new("fill_direction"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("fill_strength"),
                ty: PortableAbiType::F32,
            },
            PortableStructField {
                name: SmolStr::new("ambient_color"),
                ty: PortableAbiType::Vec3,
            },
        ],
    }
}

#[cfg(test)]
pub(super) fn temporal_resolve_input_abi() -> PortableAbiType {
    PortableAbiType::Struct {
        name: SmolStr::new("TemporalResolveInput"),
        class_id: 0,
        fields: vec![
            PortableStructField {
                name: SmolStr::new("current_color"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("history_color"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("clamp_min"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("clamp_max"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("use_history"),
                ty: PortableAbiType::Bool,
            },
        ],
    }
}

pub(super) fn attachment_dims(
    arena: &crate::presentation_exec::gpu_resources::GpuAttachmentArena,
    name: Option<&str>,
) -> (u32, u32) {
    name.and_then(|attachment| arena.attachment(attachment))
        .map(|slot| (slot.layout.width.max(1), slot.layout.height.max(1)))
        .unwrap_or((1, 1))
}

pub(super) fn shade_primary_gpu_config_abi() -> PortableAbiType {
    PortableAbiType::Struct {
        name: SmolStr::new("ShadePrimaryGpuConfig"),
        class_id: 0,
        fields: vec![
            PortableStructField {
                name: SmolStr::new("item_count"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("viewport_width"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("viewport_height"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("surface_width"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("surface_height"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("radiance_width"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("radiance_height"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("medium_width"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("medium_height"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("camera_position"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("forward"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("up"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("vertical_fov_degrees"),
                ty: PortableAbiType::F32,
            },
            PortableStructField {
                name: SmolStr::new("jitter"),
                ty: PortableAbiType::Vec2,
            },
            PortableStructField {
                name: SmolStr::new("legacy_world_up"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("legacy_view_scale"),
                ty: PortableAbiType::F32,
            },
            PortableStructField {
                name: SmolStr::new("legacy_active"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("radiance_active"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("medium_active"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("lighting"),
                ty: lighting_inputs_abi(),
            },
        ],
    }
}

pub(super) fn shade_primary_gpu_config_value(
    camera: crate::presentation_contract::CanonicalCameraInput,
    viewport: crate::presentation_contract::CanonicalViewportInput,
    jitter_pixels: [f32; 2],
    legacy_projection: bool,
    lighting: &crate::presentation_contract::PresentationLightingInputs,
    arena: &crate::presentation_exec::gpu_resources::GpuAttachmentArena,
    contract: &ShadePrimaryPassContract,
) -> KernelValue {
    let compatibility = crate::presentation_contract::LegacyCompatibilityProjectionInput {
        world_up: camera.up,
        view_scale: 0.72,
    };
    let (surface_width, surface_height) =
        attachment_dims(arena, Some(contract.surface_attachment.as_str()));
    let (radiance_width, radiance_height) =
        attachment_dims(arena, contract.radiance_attachment.as_deref());
    let (medium_width, medium_height) =
        attachment_dims(arena, contract.medium_attachment.as_deref());
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("ShadePrimaryGpuConfig"),
        fields: vec![
            (
                SmolStr::new("item_count"),
                KernelValue::U32(viewport.width.saturating_mul(viewport.height)),
            ),
            (
                SmolStr::new("viewport_width"),
                KernelValue::U32(viewport.width),
            ),
            (
                SmolStr::new("viewport_height"),
                KernelValue::U32(viewport.height),
            ),
            (
                SmolStr::new("surface_width"),
                KernelValue::U32(surface_width),
            ),
            (
                SmolStr::new("surface_height"),
                KernelValue::U32(surface_height),
            ),
            (
                SmolStr::new("radiance_width"),
                KernelValue::U32(radiance_width),
            ),
            (
                SmolStr::new("radiance_height"),
                KernelValue::U32(radiance_height),
            ),
            (SmolStr::new("medium_width"), KernelValue::U32(medium_width)),
            (
                SmolStr::new("medium_height"),
                KernelValue::U32(medium_height),
            ),
            (
                SmolStr::new("camera_position"),
                KernelValue::Vec3(camera.position),
            ),
            (SmolStr::new("forward"), KernelValue::Vec3(camera.forward)),
            (SmolStr::new("up"), KernelValue::Vec3(camera.up)),
            (
                SmolStr::new("vertical_fov_degrees"),
                KernelValue::F32(camera.vertical_fov_degrees),
            ),
            (SmolStr::new("jitter"), KernelValue::Vec2(jitter_pixels)),
            (
                SmolStr::new("legacy_world_up"),
                KernelValue::Vec3(compatibility.world_up),
            ),
            (
                SmolStr::new("legacy_view_scale"),
                KernelValue::F32(compatibility.view_scale),
            ),
            (
                SmolStr::new("legacy_active"),
                KernelValue::U32(u32::from(legacy_projection)),
            ),
            (
                SmolStr::new("radiance_active"),
                KernelValue::U32(u32::from(contract.radiance_attachment.is_some())),
            ),
            (
                SmolStr::new("medium_active"),
                KernelValue::U32(u32::from(contract.medium_attachment.is_some())),
            ),
            (SmolStr::new("lighting"), lighting_inputs_value(*lighting)),
        ],
    })
}

pub(super) fn motion_resolve_gpu_config_abi() -> PortableAbiType {
    PortableAbiType::Struct {
        name: SmolStr::new("MotionResolveGpuConfig"),
        class_id: 0,
        fields: vec![
            PortableStructField {
                name: SmolStr::new("item_count"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("viewport_width"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("viewport_height"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("previous_viewport_width"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("previous_viewport_height"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("previous_camera_position"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("previous_forward"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("previous_up"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("previous_vertical_fov_degrees"),
                ty: PortableAbiType::F32,
            },
            PortableStructField {
                name: SmolStr::new("previous_jitter"),
                ty: PortableAbiType::Vec2,
            },
            PortableStructField {
                name: SmolStr::new("history_available"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("history_rejected"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("has_history_primary_hit"),
                ty: PortableAbiType::U32,
            },
        ],
    }
}

pub(super) fn motion_resolve_gpu_config_value(
    viewport: crate::presentation_contract::CanonicalViewportInput,
    previous_camera: crate::presentation_contract::CanonicalCameraInput,
    previous_viewport: crate::presentation_contract::CanonicalViewportInput,
    previous_jitter: [f32; 2],
    history_available: bool,
    history_rejected: bool,
    has_history_primary_hit: bool,
) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("MotionResolveGpuConfig"),
        fields: vec![
            (
                SmolStr::new("item_count"),
                KernelValue::U32(viewport.width.saturating_mul(viewport.height)),
            ),
            (
                SmolStr::new("viewport_width"),
                KernelValue::U32(viewport.width),
            ),
            (
                SmolStr::new("viewport_height"),
                KernelValue::U32(viewport.height),
            ),
            (
                SmolStr::new("previous_viewport_width"),
                KernelValue::U32(previous_viewport.width),
            ),
            (
                SmolStr::new("previous_viewport_height"),
                KernelValue::U32(previous_viewport.height),
            ),
            (
                SmolStr::new("previous_camera_position"),
                KernelValue::Vec3(previous_camera.position),
            ),
            (
                SmolStr::new("previous_forward"),
                KernelValue::Vec3(previous_camera.forward),
            ),
            (
                SmolStr::new("previous_up"),
                KernelValue::Vec3(previous_camera.up),
            ),
            (
                SmolStr::new("previous_vertical_fov_degrees"),
                KernelValue::F32(previous_camera.vertical_fov_degrees),
            ),
            (
                SmolStr::new("previous_jitter"),
                KernelValue::Vec2(previous_jitter),
            ),
            (
                SmolStr::new("history_available"),
                KernelValue::U32(u32::from(history_available)),
            ),
            (
                SmolStr::new("history_rejected"),
                KernelValue::U32(u32::from(history_rejected)),
            ),
            (
                SmolStr::new("has_history_primary_hit"),
                KernelValue::U32(u32::from(has_history_primary_hit)),
            ),
        ],
    })
}

pub(super) fn temporal_resolve_gpu_config_abi() -> PortableAbiType {
    PortableAbiType::Struct {
        name: SmolStr::new("TemporalResolveGpuConfig"),
        class_id: 0,
        fields: vec![
            PortableStructField {
                name: SmolStr::new("item_count"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("width"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("height"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("history_weight_numerator"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("history_weight_denominator"),
                ty: PortableAbiType::U32,
            },
        ],
    }
}

pub(super) fn temporal_resolve_gpu_config_value(
    width: u32,
    height: u32,
    contract: &TemporalResolvePassContract,
) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("TemporalResolveGpuConfig"),
        fields: vec![
            (
                SmolStr::new("item_count"),
                KernelValue::U32(width.saturating_mul(height)),
            ),
            (SmolStr::new("width"), KernelValue::U32(width)),
            (SmolStr::new("height"), KernelValue::U32(height)),
            (
                SmolStr::new("history_weight_numerator"),
                KernelValue::U32(contract.history_weight_numerator),
            ),
            (
                SmolStr::new("history_weight_denominator"),
                KernelValue::U32(contract.history_weight_denominator),
            ),
        ],
    })
}
