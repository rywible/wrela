//! Owns attachment/resource allocation, encoding, and history-aware resource
//! helpers for presentation execution.
//! Does not own pass scheduling or shader execution.
//!
//! Key invariants:
//! - resource allocation must preserve attachment schema and history semantics
//!   from the active frame contract.
//! - encoding/decoding helpers here are part of the runtime truth surface, not
//!   debug-only conveniences.
//!
//! Primary entrypoints:
//! - attachment/resource helpers in this module
//!
//! Failure modes / common pitfalls:
//! - reusing an attachment with the wrong schema or history lifetime silently
//!   invalidates downstream passes.

use crate::artifact_layout::{PhysicalLayoutPlan, PhysicalLayoutStrategy};
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
use std::ops::Range;
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
    #[error(
        "attachment '{attachment}' expected dense shader output of {expected} bytes but received {actual}"
    )]
    DenseOutputSizeMismatch {
        attachment: SmolStr,
        expected: usize,
        actual: usize,
    },
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
pub struct FrameAttachmentLayoutMeaning {
    pub attachment: FrameAttachmentContract,
    pub element_abi: PortableAbiType,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameAttachmentLayoutPlan {
    pub meaning: FrameAttachmentLayoutMeaning,
    pub physical: PhysicalLayoutPlan,
    pub element_size: u32,
    pub wgsl_storage_type: String,
}

impl FrameAttachmentLayoutPlan {
    pub fn materialize(&self) -> FrameAttachmentLayout {
        FrameAttachmentLayout {
            attachment: self.meaning.attachment.clone(),
            width: self.physical.width,
            height: self.physical.height,
            element_abi: self.meaning.element_abi.clone(),
            element_size: self.element_size,
            element_stride: self.physical.element_stride,
            total_size: self.physical.total_size,
            wgsl_storage_type: self.wgsl_storage_type.clone(),
            plan: self.clone(),
        }
    }

    pub fn dense_output_size(&self) -> usize {
        self.physical.element_count as usize * self.physical.element_stride as usize
    }

    pub fn compatibility_signature(&self) -> u64 {
        let kind = format!("{:?}", self.meaning.attachment.kind);
        let element_schema = format!("{:?}", self.meaning.attachment.element_schema);
        let lifetime = format!("{:?}", self.meaning.attachment.lifetime);
        let resolution = format!("{:?}", self.meaning.attachment.resolution);
        let element_abi = format!("{:?}", self.meaning.element_abi);
        let strategy = format!("{:?}", self.physical.strategy);
        crate::query_exec::ids::stable_semantic_id(&[
            kind.as_bytes(),
            element_schema.as_bytes(),
            lifetime.as_bytes(),
            resolution.as_bytes(),
            element_abi.as_bytes(),
            &self.meaning.attachment.scale.divisor_x.to_le_bytes(),
            &self.meaning.attachment.scale.divisor_y.to_le_bytes(),
            &self.meaning.width.to_le_bytes(),
            &self.meaning.height.to_le_bytes(),
            &self.physical.element_stride.to_le_bytes(),
            &self.physical.row_stride.to_le_bytes(),
            &self.physical.total_size.to_le_bytes(),
            strategy.as_bytes(),
        ])
    }

    pub fn pack_dense_output_bytes(
        &self,
        dense_bytes: &[u8],
    ) -> Result<Vec<u8>, PresentationResourceError> {
        let expected = self.dense_output_size();
        if dense_bytes.len() != expected {
            return Err(PresentationResourceError::DenseOutputSizeMismatch {
                attachment: self.meaning.attachment.name.clone(),
                expected,
                actual: dense_bytes.len(),
            });
        }
        if self.physical.row_stride
            == self
                .physical
                .width
                .saturating_mul(self.physical.element_stride)
        {
            return Ok(dense_bytes.to_vec());
        }

        let mut bytes = vec![0; self.physical.total_size as usize];
        let stride = self.physical.element_stride as usize;
        for index in 0..self.physical.element_count as usize {
            let dense_start = index * stride;
            let dense_end = dense_start + stride;
            let range = layout_element_range(self, index)?;
            bytes[range].copy_from_slice(&dense_bytes[dense_start..dense_end]);
        }
        Ok(bytes)
    }
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
    pub plan: FrameAttachmentLayoutPlan,
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
    allocate_attachment_resources_with_history_and_strategy(
        frame,
        width,
        height,
        previous,
        PhysicalLayoutStrategy::DenseBuffer,
    )
}

pub fn allocate_attachment_resources_with_history_and_strategy(
    frame: &FrameContract,
    width: u32,
    height: u32,
    previous: Option<&AttachmentResourceSet>,
    strategy: PhysicalLayoutStrategy,
) -> Result<AttachmentResourceSet, PresentationResourceError> {
    let mut attachments = BTreeMap::new();
    for attachment in &frame.outputs {
        let layout_plan =
            frame_attachment_layout_plan_with_strategy(frame, attachment, width, height, strategy)?;
        let bytes = initialize_attachment_bytes(attachment, &layout_plan, previous)?;
        attachments.insert(
            attachment.name.clone(),
            AttachmentResource {
                bytes,
                layout: layout_plan.materialize(),
            },
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
    frame_attachment_layout_plan(frame, attachment, width, height).map(|plan| plan.materialize())
}

pub fn frame_attachment_layout_plan(
    frame: &FrameContract,
    attachment: &FrameAttachmentContract,
    width: u32,
    height: u32,
) -> Result<FrameAttachmentLayoutPlan, PresentationResourceError> {
    frame_attachment_layout_plan_with_strategy(
        frame,
        attachment,
        width,
        height,
        PhysicalLayoutStrategy::DenseBuffer,
    )
}

pub fn frame_attachment_layout_plan_with_strategy(
    frame: &FrameContract,
    attachment: &FrameAttachmentContract,
    width: u32,
    height: u32,
    strategy: PhysicalLayoutStrategy,
) -> Result<FrameAttachmentLayoutPlan, PresentationResourceError> {
    let element_abi = attachment_element_abi(frame, attachment)?;
    let element_size = portable_abi_layout(&element_abi).size;
    let element_stride = portable_abi_array_stride(&element_abi);
    let scaled_width = width.div_ceil(attachment.scale.divisor_x.max(1));
    let scaled_height = height.div_ceil(attachment.scale.divisor_y.max(1));
    let wgsl_storage_type = portable_abi_wgsl_type_name(&element_abi)?;
    let physical = match strategy {
        PhysicalLayoutStrategy::DenseBuffer => {
            PhysicalLayoutPlan::dense_buffer(scaled_width, scaled_height, element_stride)
        }
        PhysicalLayoutStrategy::RowAligned { row_alignment } => PhysicalLayoutPlan::row_aligned(
            scaled_width,
            scaled_height,
            element_stride,
            row_alignment,
        ),
    };
    Ok(FrameAttachmentLayoutPlan {
        meaning: FrameAttachmentLayoutMeaning {
            attachment: attachment.clone(),
            element_abi,
            width: scaled_width,
            height: scaled_height,
        },
        physical,
        element_size,
        wgsl_storage_type,
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

pub fn attachment_policy_description(attachment: &FrameAttachmentContract) -> String {
    attachment.policy_description()
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
        self.layout.plan.physical.element_count as usize
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
        layout_element_range(&self.layout.plan, index)
    }
}

impl FrameAttachmentLayout {
    pub fn compatibility_signature(&self) -> u64 {
        self.plan.compatibility_signature()
    }
}

fn initialize_attachment_bytes(
    attachment: &FrameAttachmentContract,
    layout: &FrameAttachmentLayoutPlan,
    previous: Option<&AttachmentResourceSet>,
) -> Result<Vec<u8>, PresentationResourceError> {
    match attachment.clear_policy {
        AttachmentClearPolicy::Zero => Ok(vec![0; layout.physical.total_size as usize]),
        AttachmentClearPolicy::SemanticDefault => {
            let mut bytes = vec![0; layout.physical.total_size as usize];
            let encoded = portable_abi_encode_value(
                &layout.meaning.element_abi,
                &semantic_default_value(attachment),
            )?;
            for index in 0..layout.physical.element_count as usize {
                let range = layout_element_range(layout, index)?;
                bytes[range.clone()].fill(0);
                let copy_len = encoded.len().min(range.len());
                bytes[range.start..range.start + copy_len].copy_from_slice(&encoded[..copy_len]);
            }
            Ok(bytes)
        }
        AttachmentClearPolicy::PreservePrevious => {
            let prior = previous
                .and_then(|resources| resources.attachment(attachment.name.as_str()))
                .ok_or_else(|| PresentationResourceError::MissingHistoryAttachment {
                    attachment: attachment.name.clone(),
                })?;
            if prior.layout.plan.compatibility_signature() != layout.compatibility_signature() {
                return Err(PresentationResourceError::HistoryLayoutMismatch {
                    attachment: attachment.name.clone(),
                });
            }
            Ok(prior.bytes.clone())
        }
    }
}

fn layout_element_range(
    layout: &FrameAttachmentLayoutPlan,
    index: usize,
) -> Result<Range<usize>, PresentationResourceError> {
    if index >= layout.physical.element_count as usize {
        return Err(PresentationResourceError::IndexOutOfBounds {
            attachment: layout.meaning.attachment.name.clone(),
            index,
            len: layout.physical.element_count as usize,
        });
    }

    let width = layout.physical.width as usize;
    if width == 0 {
        return Ok(0..0);
    }

    let row = index / width;
    let column = index % width;
    let start = row * layout.physical.row_stride as usize
        + column * layout.physical.element_stride as usize;
    let end = start + layout.physical.element_stride as usize;
    Ok(start..end)
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
