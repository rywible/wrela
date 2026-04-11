use crate::kernel::{KernelStructValue, KernelValue};
use crate::portable::{
    PortableAbiError, PortableAbiType, portable_abi_array_stride, portable_abi_decode_value,
    portable_abi_encode_value, portable_abi_layout, portable_abi_wgsl_type_name,
    portable_builtin_record_abi,
};
use crate::presentation_contract::{
    AttachmentClearPolicy, AttachmentElementSchema, FrameAttachmentContract, FrameAttachmentKind,
    FrameContract,
};
use crate::query_exec::cpu::{default_medium, default_surface};
use smol_str::SmolStr;
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PresentationResourceError {
    #[error("attachment '{attachment}' references unknown builtin record '{record}'")]
    UnknownBuiltinRecord {
        attachment: SmolStr,
        record: SmolStr,
    },
    #[error("attachment '{attachment}' is not allocated")]
    MissingAttachment { attachment: SmolStr },
    #[error("history attachment '{attachment}' requires previous frame contents")]
    MissingHistoryAttachment { attachment: SmolStr },
    #[error("history attachment '{attachment}' does not match prior layout")]
    HistoryLayoutMismatch { attachment: SmolStr },
    #[error("attachment '{attachment}' index {index} is out of bounds for {len} elements")]
    IndexOutOfBounds {
        attachment: SmolStr,
        index: usize,
        len: usize,
    },
    #[error(transparent)]
    Portable(#[from] PortableAbiError),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameAttachmentLayout {
    pub attachment: FrameAttachmentContract,
    pub width: u32,
    pub height: u32,
    pub element_abi: PortableAbiType,
    pub element_size: u32,
    pub element_stride: u32,
    pub total_size: u32,
    pub wgsl_storage_type: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttachmentResource {
    pub layout: FrameAttachmentLayout,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttachmentResourceSet {
    pub width: u32,
    pub height: u32,
    pub attachments: BTreeMap<SmolStr, AttachmentResource>,
}

pub fn allocate_attachment_resources(
    frame: &FrameContract,
    width: u32,
    height: u32,
) -> Result<AttachmentResourceSet, PresentationResourceError> {
    allocate_attachment_resources_with_history(frame, width, height, None)
}

pub fn allocate_attachment_resources_without_history(
    frame: &FrameContract,
    width: u32,
    height: u32,
) -> Result<AttachmentResourceSet, PresentationResourceError> {
    let mut fresh_frame = frame.clone();
    for attachment in &mut fresh_frame.outputs {
        if attachment.clear_policy == AttachmentClearPolicy::PreservePrevious {
            attachment.clear_policy = AttachmentClearPolicy::SemanticDefault;
        }
    }
    allocate_attachment_resources_with_history(&fresh_frame, width, height, None)
}

pub fn allocate_attachment_resources_with_history(
    frame: &FrameContract,
    width: u32,
    height: u32,
    previous: Option<&AttachmentResourceSet>,
) -> Result<AttachmentResourceSet, PresentationResourceError> {
    let mut attachments = BTreeMap::new();
    for attachment in &frame.outputs {
        let layout = frame_attachment_layout(frame, attachment, width, height)?;
        let bytes = initialize_attachment_bytes(attachment, &layout, previous)?;
        attachments.insert(
            attachment.name.clone(),
            AttachmentResource { bytes, layout },
        );
    }
    Ok(AttachmentResourceSet {
        width,
        height,
        attachments,
    })
}

pub fn frame_attachment_layout(
    frame: &FrameContract,
    attachment: &FrameAttachmentContract,
    width: u32,
    height: u32,
) -> Result<FrameAttachmentLayout, PresentationResourceError> {
    let _ = frame;
    let element_abi = attachment_element_abi(frame, attachment)?;
    let element_size = portable_abi_layout(&element_abi).size;
    let element_stride = portable_abi_array_stride(&element_abi);
    let scaled_width = width.div_ceil(attachment.scale.divisor_x.max(1));
    let scaled_height = height.div_ceil(attachment.scale.divisor_y.max(1));
    let element_count = scaled_width.saturating_mul(scaled_height);
    Ok(FrameAttachmentLayout {
        attachment: attachment.clone(),
        width: scaled_width,
        height: scaled_height,
        element_size,
        element_stride,
        total_size: element_stride.saturating_mul(element_count),
        wgsl_storage_type: portable_abi_wgsl_type_name(&element_abi)?,
        element_abi,
    })
}

pub fn attachment_element_abi(
    _frame: &FrameContract,
    attachment: &FrameAttachmentContract,
) -> Result<PortableAbiType, PresentationResourceError> {
    match &attachment.element_schema {
        AttachmentElementSchema::NamedRecord(record) => {
            portable_builtin_record_abi(record.as_str()).ok_or_else(|| {
                PresentationResourceError::UnknownBuiltinRecord {
                    attachment: attachment.name.clone(),
                    record: record.clone(),
                }
            })
        }
        AttachmentElementSchema::ScalarF32 => Ok(PortableAbiType::F32),
        AttachmentElementSchema::Vec2F32 => Ok(PortableAbiType::Vec2),
        AttachmentElementSchema::Vec3F32 => Ok(PortableAbiType::Vec3),
        AttachmentElementSchema::Vec4F32 => Ok(PortableAbiType::Vec4),
    }
}

impl AttachmentResourceSet {
    pub fn attachment(&self, name: &str) -> Option<&AttachmentResource> {
        self.attachments.get(name)
    }

    pub fn attachment_mut(&mut self, name: &str) -> Option<&mut AttachmentResource> {
        self.attachments.get_mut(name)
    }

    pub fn decode_attachment(
        &self,
        name: &str,
    ) -> Result<Vec<KernelValue>, PresentationResourceError> {
        let attachment =
            self.attachment(name)
                .ok_or_else(|| PresentationResourceError::MissingAttachment {
                    attachment: SmolStr::new(name),
                })?;
        (0..attachment.element_count())
            .map(|index| attachment.decode(index))
            .collect()
    }
}

impl AttachmentResource {
    pub fn element_count(&self) -> usize {
        self.layout.width.saturating_mul(self.layout.height) as usize
    }

    pub fn encode(
        &mut self,
        index: usize,
        value: &KernelValue,
    ) -> Result<(), PresentationResourceError> {
        let range = self.element_range(index)?;
        let encoded = portable_abi_encode_value(&self.layout.element_abi, value)?;
        self.bytes[range.clone()].fill(0);
        let copy_len = encoded.len().min(range.len());
        self.bytes[range.start..range.start + copy_len].copy_from_slice(&encoded[..copy_len]);
        Ok(())
    }

    pub fn decode(&self, index: usize) -> Result<KernelValue, PresentationResourceError> {
        let range = self.element_range(index)?;
        portable_abi_decode_value(&self.layout.element_abi, &self.bytes[range])
            .map_err(PresentationResourceError::from)
    }

    fn element_range(
        &self,
        index: usize,
    ) -> Result<std::ops::Range<usize>, PresentationResourceError> {
        if index >= self.element_count() {
            return Err(PresentationResourceError::IndexOutOfBounds {
                attachment: self.layout.attachment.name.clone(),
                index,
                len: self.element_count(),
            });
        }
        let start = index * self.layout.element_stride as usize;
        let end = start + self.layout.element_stride as usize;
        Ok(start..end)
    }
}

fn initialize_attachment_bytes(
    attachment: &FrameAttachmentContract,
    layout: &FrameAttachmentLayout,
    previous: Option<&AttachmentResourceSet>,
) -> Result<Vec<u8>, PresentationResourceError> {
    match attachment.clear_policy {
        AttachmentClearPolicy::Zero => Ok(vec![0; layout.total_size as usize]),
        AttachmentClearPolicy::SemanticDefault => {
            let mut bytes = vec![0; layout.total_size as usize];
            let encoded = portable_abi_encode_value(
                &layout.element_abi,
                &semantic_default_value(attachment),
            )?;
            for index in 0..layout.width.saturating_mul(layout.height) as usize {
                let start = index * layout.element_stride as usize;
                let end = start + layout.element_stride as usize;
                bytes[start..end].fill(0);
                let copy_len = encoded.len().min(layout.element_stride as usize);
                bytes[start..start + copy_len].copy_from_slice(&encoded[..copy_len]);
            }
            Ok(bytes)
        }
        AttachmentClearPolicy::PreservePrevious => {
            let prior = previous
                .and_then(|resources| resources.attachment(attachment.name.as_str()))
                .ok_or_else(|| PresentationResourceError::MissingHistoryAttachment {
                    attachment: attachment.name.clone(),
                })?;
            if prior.layout.width != layout.width
                || prior.layout.height != layout.height
                || prior.layout.element_abi != layout.element_abi
                || prior.layout.element_stride != layout.element_stride
            {
                return Err(PresentationResourceError::HistoryLayoutMismatch {
                    attachment: attachment.name.clone(),
                });
            }
            Ok(prior.bytes.clone())
        }
    }
}

fn semantic_default_value(attachment: &FrameAttachmentContract) -> KernelValue {
    match attachment.kind {
        FrameAttachmentKind::PrimaryHit => default_primary_hit_value(),
        FrameAttachmentKind::Depth => KernelValue::F32(f32::INFINITY),
        FrameAttachmentKind::WorldNormal => KernelValue::Vec3([0.0, 0.0, 0.0]),
        FrameAttachmentKind::Surface
        | FrameAttachmentKind::Radiance
        | FrameAttachmentKind::Medium
        | FrameAttachmentKind::Motion
        | FrameAttachmentKind::Color => zero_value_for_schema(&attachment.element_schema),
    }
}

fn zero_value_for_schema(schema: &AttachmentElementSchema) -> KernelValue {
    match schema {
        AttachmentElementSchema::NamedRecord(name) if name == "Hit3" => default_primary_hit_value(),
        AttachmentElementSchema::NamedRecord(name) if name == "MotionVector" => {
            default_motion_vector_value()
        }
        AttachmentElementSchema::NamedRecord(name) if name == "Payload" => default_payload_value(),
        AttachmentElementSchema::NamedRecord(name) if name == "ActorHandle" => {
            default_actor_handle_value()
        }
        AttachmentElementSchema::NamedRecord(name) if name == "Transform3" => {
            default_transform3_value()
        }
        AttachmentElementSchema::NamedRecord(name) if name == "Surface" => default_surface(),
        AttachmentElementSchema::NamedRecord(name) if name == "Medium" => default_medium(),
        AttachmentElementSchema::NamedRecord(name) => KernelValue::Struct(KernelStructValue {
            name: name.clone(),
            fields: Vec::new(),
        }),
        AttachmentElementSchema::ScalarF32 => KernelValue::F32(0.0),
        AttachmentElementSchema::Vec2F32 => KernelValue::Vec2([0.0, 0.0]),
        AttachmentElementSchema::Vec3F32 => KernelValue::Vec3([0.0, 0.0, 0.0]),
        AttachmentElementSchema::Vec4F32 => KernelValue::Vec4([0.0, 0.0, 0.0, 0.0]),
    }
}

fn default_primary_hit_value() -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("Hit3"),
        fields: vec![
            (SmolStr::new("hit"), KernelValue::Bool(false)),
            (SmolStr::new("distance"), KernelValue::F32(f32::INFINITY)),
            (SmolStr::new("position"), KernelValue::Vec3([0.0, 0.0, 0.0])),
            (SmolStr::new("normal"), KernelValue::Vec3([0.0, 0.0, 0.0])),
            (
                SmolStr::new("local_position"),
                KernelValue::Vec3([0.0, 0.0, 0.0]),
            ),
            (
                SmolStr::new("local_normal"),
                KernelValue::Vec3([0.0, 0.0, 0.0]),
            ),
            (SmolStr::new("shading_frame"), default_transform3_value()),
            (SmolStr::new("steps"), KernelValue::I32(0)),
            (SmolStr::new("feature_id"), KernelValue::U32(0)),
            (SmolStr::new("instance_id"), KernelValue::U32(0)),
            (SmolStr::new("repeat_id"), KernelValue::U32(0)),
            (SmolStr::new("root_shape_id"), KernelValue::U32(0)),
            (SmolStr::new("payload"), default_payload_value()),
        ],
    })
}

fn default_motion_vector_value() -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("MotionVector"),
        fields: vec![
            (SmolStr::new("delta_pixels"), KernelValue::Vec2([0.0, 0.0])),
            (
                SmolStr::new("previous_sample"),
                KernelValue::Vec2([0.0, 0.0]),
            ),
            (SmolStr::new("valid"), KernelValue::Bool(false)),
            (SmolStr::new("disoccluded"), KernelValue::Bool(false)),
        ],
    })
}

fn default_payload_value() -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("Payload"),
        fields: vec![
            (SmolStr::new("entity_id"), KernelValue::U32(0)),
            (SmolStr::new("material_id"), KernelValue::U32(0)),
            (SmolStr::new("actor"), default_actor_handle_value()),
        ],
    })
}

fn default_actor_handle_value() -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("ActorHandle"),
        fields: vec![
            (SmolStr::new("id"), KernelValue::U32(0)),
            (SmolStr::new("generation"), KernelValue::U32(0)),
        ],
    })
}

fn default_transform3_value() -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("Transform3"),
        fields: vec![
            (
                SmolStr::new("matrix"),
                KernelValue::Mat4([
                    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
                ]),
            ),
            (
                SmolStr::new("inverse"),
                KernelValue::Mat4([
                    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
                ]),
            ),
        ],
    })
}
