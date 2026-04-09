use smol_str::SmolStr;
use std::collections::BTreeSet;
use thiserror::Error;

use crate::kernel::{KernelStructValue, KernelValue};
use crate::query_plan::{
    ArtifactContract, ArtifactSchema, CandidateRecordContract, DispatchRecordContract,
    HitContextContract, ParticipantSelectionContract, ResultRecordContract,
};

#[derive(Debug, Clone, PartialEq)]
pub enum PortableAbiType {
    Value,
    Bool,
    I32,
    U32,
    F32,
    Vec2,
    Vec3,
    Vec4,
    Mat3,
    Mat4,
    Quat,
    Array(Box<PortableAbiType>, usize),
    Struct {
        name: SmolStr,
        class_id: u32,
        fields: Vec<PortableStructField>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct PortableStructField {
    pub name: SmolStr,
    pub ty: PortableAbiType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortableAbiLayout {
    pub size: u32,
    pub align: u32,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PortableAbiError {
    #[error("WGSL/storage ABI does not support runtime Value records")]
    UnsupportedValueType,
    #[error("expected {expected}, found {found}")]
    TypeMismatch { expected: String, found: String },
    #[error("missing field '{field}' on struct '{name}'")]
    MissingField { name: SmolStr, field: SmolStr },
    #[error("expected array length {expected}, found {found}")]
    ArrayLengthMismatch { expected: usize, found: usize },
    #[error("buffer too small: expected at least {expected} bytes, found {found}")]
    BufferTooSmall { expected: usize, found: usize },
}

impl PortableAbiLayout {
    pub const fn new(size: u32, align: u32) -> Self {
        Self { size, align }
    }
}

pub fn portable_abi_layout(abi: &PortableAbiType) -> PortableAbiLayout {
    match abi {
        PortableAbiType::Bool => PortableAbiLayout::new(4, 4),
        PortableAbiType::I32 | PortableAbiType::U32 | PortableAbiType::F32 => {
            PortableAbiLayout::new(4, 4)
        }
        PortableAbiType::Vec2 => PortableAbiLayout::new(8, 8),
        PortableAbiType::Vec3 => PortableAbiLayout::new(12, 16),
        PortableAbiType::Vec4 | PortableAbiType::Quat => PortableAbiLayout::new(16, 16),
        PortableAbiType::Mat3 => PortableAbiLayout::new(48, 16),
        PortableAbiType::Mat4 => PortableAbiLayout::new(64, 16),
        PortableAbiType::Array(inner, len) => {
            let stride = portable_abi_array_stride(inner);
            let size = stride.saturating_mul(*len as u32);
            PortableAbiLayout::new(size, portable_abi_layout(inner).align.max(1))
        }
        PortableAbiType::Struct { fields, .. } => {
            let mut offset = 0;
            let mut max_align = 1;
            for field in fields {
                let layout = portable_abi_layout(&field.ty);
                max_align = max_align.max(layout.align);
                offset = align_to_u32(offset, layout.align);
                offset += layout.size;
            }
            PortableAbiLayout::new(align_to_u32(offset, max_align).max(1), max_align.max(1))
        }
        PortableAbiType::Value => PortableAbiLayout::new(8, 8),
    }
}

pub fn portable_abi_array_stride(abi: &PortableAbiType) -> u32 {
    let layout = portable_abi_layout(abi);
    align_to_u32(layout.size, layout.align.max(1))
}

pub fn portable_abi_field_offset(fields: &[PortableStructField], index: usize) -> u32 {
    let mut offset = 0;
    for field in fields.iter().take(index) {
        let layout = portable_abi_layout(&field.ty);
        offset = align_to_u32(offset, layout.align);
        offset += layout.size;
    }
    if let Some(field) = fields.get(index) {
        align_to_u32(offset, portable_abi_layout(&field.ty).align)
    } else {
        offset
    }
}

pub fn portable_abi_lane_offset(abi: &PortableAbiType, index: usize) -> Option<u32> {
    match abi {
        PortableAbiType::Vec2 if index < 2 => Some(index as u32 * 4),
        PortableAbiType::Vec3 if index < 3 => Some(index as u32 * 4),
        PortableAbiType::Vec4 | PortableAbiType::Quat if index < 4 => Some(index as u32 * 4),
        PortableAbiType::Mat3 if index < 9 => {
            let column = index / 3;
            let row = index % 3;
            Some(column as u32 * 16 + row as u32 * 4)
        }
        PortableAbiType::Mat4 if index < 16 => Some(index as u32 * 4),
        _ => None,
    }
}

pub fn align_to_u32(offset: u32, align: u32) -> u32 {
    if align <= 1 {
        return offset;
    }
    let rem = offset % align;
    if rem == 0 {
        offset
    } else {
        offset + (align - rem)
    }
}

fn abi_field(name: &str, ty: PortableAbiType) -> PortableStructField {
    PortableStructField {
        name: SmolStr::new(name),
        ty,
    }
}

pub fn portable_dispatch_contract_abi(contract: &DispatchRecordContract) -> PortableAbiType {
    let _ = contract;
    PortableAbiType::Struct {
        name: SmolStr::new("DispatchRecordContract"),
        class_id: 0,
        fields: vec![
            abi_field("backend", PortableAbiType::U32),
            abi_field("kernel", PortableAbiType::U32),
            abi_field("item_kind", PortableAbiType::U32),
            abi_field("result_kind", PortableAbiType::U32),
            abi_field("contract_version", PortableAbiType::U32),
        ],
    }
}

pub fn portable_result_contract_abi(contract: &ResultRecordContract) -> PortableAbiType {
    let _ = contract;
    PortableAbiType::Struct {
        name: SmolStr::new("ResultRecordContract"),
        class_id: 0,
        fields: vec![
            abi_field("result_kind", PortableAbiType::U32),
            abi_field("preserves_local_hit_context", PortableAbiType::Bool),
            abi_field("stable_feature_id", PortableAbiType::Bool),
            abi_field("stable_instance_id", PortableAbiType::Bool),
            abi_field("stable_repeat_id", PortableAbiType::Bool),
            abi_field("contract_version", PortableAbiType::U32),
        ],
    }
}

pub fn portable_candidate_contract_abi(contract: &CandidateRecordContract) -> PortableAbiType {
    let _ = contract;
    PortableAbiType::Struct {
        name: SmolStr::new("CandidateRecordContract"),
        class_id: 0,
        fields: vec![
            abi_field("source", PortableAbiType::U32),
            abi_field("item_kind", PortableAbiType::U32),
            abi_field("candidate_strategy", PortableAbiType::U32),
            abi_field("pruning_strategy", PortableAbiType::U32),
            abi_field("winner_mode", PortableAbiType::U32),
            abi_field("stable_leaf_identity", PortableAbiType::Bool),
            abi_field("contract_version", PortableAbiType::U32),
        ],
    }
}

pub fn portable_hit_context_contract_abi(contract: &HitContextContract) -> PortableAbiType {
    let _ = contract;
    PortableAbiType::Struct {
        name: SmolStr::new("HitContextContract"),
        class_id: 0,
        fields: vec![
            abi_field("world_position", PortableAbiType::Bool),
            abi_field("world_normal", PortableAbiType::Bool),
            abi_field("local_position", PortableAbiType::Bool),
            abi_field("local_normal", PortableAbiType::Bool),
            abi_field("shading_frame", PortableAbiType::Bool),
            abi_field("payload", PortableAbiType::Bool),
            abi_field("contract_version", PortableAbiType::U32),
        ],
    }
}

pub fn portable_participant_contract_abi(
    contract: &ParticipantSelectionContract,
) -> PortableAbiType {
    let _ = contract;
    PortableAbiType::Struct {
        name: SmolStr::new("ParticipantSelectionContract"),
        class_id: 0,
        fields: vec![
            abi_field("kind", PortableAbiType::U32),
            abi_field("provenance_aware", PortableAbiType::Bool),
            abi_field("additive", PortableAbiType::Bool),
            abi_field("contract_version", PortableAbiType::U32),
        ],
    }
}

pub fn portable_artifact_contract_abi(contract: &ArtifactContract) -> PortableAbiType {
    let mut fields = vec![
        abi_field("schema_kind", PortableAbiType::U32),
        abi_field("version", PortableAbiType::U32),
        abi_field("deterministic", PortableAbiType::Bool),
    ];
    match &contract.schema {
        ArtifactSchema::SupportSummary {
            semantic_root,
            support_root,
            node_count,
            support_node_count,
            leaf_count,
            identity_source_count,
            ..
        } => {
            let _ = (
                semantic_root,
                support_root,
                node_count,
                support_node_count,
                leaf_count,
                identity_source_count,
            );
            fields.extend([
                abi_field("semantics", PortableAbiType::U32),
                abi_field("support_class", PortableAbiType::U32),
                abi_field("can_coarse_support_pruning", PortableAbiType::Bool),
                abi_field("semantic_root", PortableAbiType::U32),
                abi_field("support_root", PortableAbiType::U32),
                abi_field("node_count", PortableAbiType::U32),
                abi_field("support_node_count", PortableAbiType::U32),
                abi_field("leaf_count", PortableAbiType::U32),
                abi_field("identity_source_count", PortableAbiType::U32),
            ]);
        }
        ArtifactSchema::CaptureCache { .. } => {
            fields.extend([
                abi_field("capture_kind", PortableAbiType::U32),
                abi_field("semantic_root", PortableAbiType::U32),
            ]);
        }
        ArtifactSchema::CullingTable { .. } => {
            fields.extend([
                abi_field("candidate_strategy", PortableAbiType::U32),
                abi_field("pruning_strategy", PortableAbiType::U32),
                abi_field("support_class", PortableAbiType::U32),
                abi_field("semantics", PortableAbiType::U32),
                abi_field("support_root", PortableAbiType::U32),
                abi_field("support_node_count", PortableAbiType::U32),
                abi_field("leaf_count", PortableAbiType::U32),
                abi_field("identity_source_count", PortableAbiType::U32),
            ]);
        }
        ArtifactSchema::DispatchRecord { .. } => {
            fields.extend([
                abi_field("item_kind", PortableAbiType::U32),
                abi_field("result_kind", PortableAbiType::U32),
            ]);
        }
        ArtifactSchema::HitResultBuffer { .. } => {
            fields.extend([
                abi_field("result_kind", PortableAbiType::U32),
                abi_field("preserves_local_hit_context", PortableAbiType::Bool),
            ]);
        }
        ArtifactSchema::OpaquePessimizationBoundary { .. } => {
            fields.extend([
                abi_field("support_root", PortableAbiType::U32),
                abi_field("support_node_count", PortableAbiType::U32),
            ]);
        }
    }
    PortableAbiType::Struct {
        name: SmolStr::new("ArtifactContract"),
        class_id: 0,
        fields,
    }
}

pub fn portable_abi_encode_value(
    abi: &PortableAbiType,
    value: &KernelValue,
) -> Result<Vec<u8>, PortableAbiError> {
    let mut bytes = vec![0u8; portable_abi_layout(abi).size as usize];
    portable_abi_write_value(abi, value, &mut bytes)?;
    Ok(bytes)
}

pub fn portable_abi_write_value(
    abi: &PortableAbiType,
    value: &KernelValue,
    bytes: &mut [u8],
) -> Result<(), PortableAbiError> {
    let expected = portable_abi_layout(abi).size as usize;
    if bytes.len() < expected {
        return Err(PortableAbiError::BufferTooSmall {
            expected,
            found: bytes.len(),
        });
    }
    write_portable_abi_value_at(abi, value, bytes, 0)
}

pub fn portable_abi_decode_value(
    abi: &PortableAbiType,
    bytes: &[u8],
) -> Result<KernelValue, PortableAbiError> {
    let expected = portable_abi_layout(abi).size as usize;
    if bytes.len() < expected {
        return Err(PortableAbiError::BufferTooSmall {
            expected,
            found: bytes.len(),
        });
    }
    read_portable_abi_value_at(abi, bytes, 0)
}

pub fn portable_abi_encode_slice(
    abi: &PortableAbiType,
    values: &[KernelValue],
) -> Result<Vec<u8>, PortableAbiError> {
    let stride = portable_abi_array_stride(abi) as usize;
    let mut bytes = vec![0u8; stride.saturating_mul(values.len())];
    for (index, value) in values.iter().enumerate() {
        write_portable_abi_value_at(abi, value, &mut bytes, index * stride)?;
    }
    Ok(bytes)
}

pub fn portable_abi_decode_slice(
    abi: &PortableAbiType,
    bytes: &[u8],
    len: usize,
) -> Result<Vec<KernelValue>, PortableAbiError> {
    let stride = portable_abi_array_stride(abi) as usize;
    let expected = stride.saturating_mul(len);
    if bytes.len() < expected {
        return Err(PortableAbiError::BufferTooSmall {
            expected,
            found: bytes.len(),
        });
    }
    let mut out = Vec::with_capacity(len);
    for index in 0..len {
        out.push(read_portable_abi_value_at(abi, bytes, index * stride)?);
    }
    Ok(out)
}

pub fn portable_abi_wgsl_type_name(abi: &PortableAbiType) -> Result<String, PortableAbiError> {
    match abi {
        PortableAbiType::Value => Err(PortableAbiError::UnsupportedValueType),
        PortableAbiType::Bool => Ok("u32".to_string()),
        PortableAbiType::I32 => Ok("i32".to_string()),
        PortableAbiType::U32 => Ok("u32".to_string()),
        PortableAbiType::F32 => Ok("f32".to_string()),
        PortableAbiType::Vec2 => Ok("vec2<f32>".to_string()),
        PortableAbiType::Vec3 => Ok("vec3<f32>".to_string()),
        PortableAbiType::Vec4 | PortableAbiType::Quat => Ok("vec4<f32>".to_string()),
        PortableAbiType::Mat3 => Ok("mat3x3<f32>".to_string()),
        PortableAbiType::Mat4 => Ok("mat4x4<f32>".to_string()),
        PortableAbiType::Array(inner, len) => Ok(format!(
            "array<{}, {}>",
            portable_abi_wgsl_type_name(inner)?,
            len
        )),
        PortableAbiType::Struct { name, .. } => Ok(name.to_string()),
    }
}

pub fn portable_abi_emit_wgsl_structs(
    roots: &[PortableAbiType],
) -> Result<String, PortableAbiError> {
    let mut emitted = Vec::new();
    let mut seen = BTreeSet::new();
    for root in roots {
        collect_wgsl_structs(root, &mut seen, &mut emitted)?;
    }
    Ok(emitted.join("\n\n"))
}

fn collect_wgsl_structs(
    abi: &PortableAbiType,
    seen: &mut BTreeSet<SmolStr>,
    emitted: &mut Vec<String>,
) -> Result<(), PortableAbiError> {
    match abi {
        PortableAbiType::Value => Err(PortableAbiError::UnsupportedValueType),
        PortableAbiType::Array(inner, _) => collect_wgsl_structs(inner, seen, emitted),
        PortableAbiType::Struct { name, fields, .. } => {
            if seen.contains(name) {
                return Ok(());
            }
            for field in fields {
                collect_wgsl_structs(&field.ty, seen, emitted)?;
            }
            seen.insert(name.clone());
            let mut rendered = format!("struct {} {{\n", name);
            for field in fields {
                rendered.push_str(&format!(
                    "  {}: {},\n",
                    field.name,
                    portable_abi_wgsl_type_name(&field.ty)?
                ));
            }
            rendered.push('}');
            emitted.push(rendered);
            Ok(())
        }
        _ => Ok(()),
    }
}

fn write_portable_abi_value_at(
    abi: &PortableAbiType,
    value: &KernelValue,
    bytes: &mut [u8],
    base_offset: usize,
) -> Result<(), PortableAbiError> {
    ensure_capacity(abi, bytes, base_offset)?;
    match (abi, value) {
        (PortableAbiType::Value, _) => Err(PortableAbiError::UnsupportedValueType),
        (PortableAbiType::Bool, KernelValue::Bool(flag)) => {
            write_u32(bytes, base_offset, u32::from(*flag))
        }
        (PortableAbiType::I32, KernelValue::I32(number)) => write_i32(bytes, base_offset, *number),
        (PortableAbiType::U32, KernelValue::U32(number)) => write_u32(bytes, base_offset, *number),
        (PortableAbiType::F32, KernelValue::F32(number)) => write_f32(bytes, base_offset, *number),
        (PortableAbiType::Vec2, KernelValue::Vec2(values)) => {
            write_f32(bytes, base_offset, values[0])?;
            write_f32(bytes, base_offset + 4, values[1])
        }
        (PortableAbiType::Vec3, KernelValue::Vec3(values)) => {
            for (index, value) in values.iter().enumerate() {
                write_f32(
                    bytes,
                    base_offset + portable_abi_lane_offset(abi, index).unwrap() as usize,
                    *value,
                )?;
            }
            Ok(())
        }
        (PortableAbiType::Vec4, KernelValue::Vec4(values))
        | (PortableAbiType::Quat, KernelValue::Quat(values)) => {
            for (index, value) in values.iter().enumerate() {
                write_f32(
                    bytes,
                    base_offset + portable_abi_lane_offset(abi, index).unwrap() as usize,
                    *value,
                )?;
            }
            Ok(())
        }
        (PortableAbiType::Mat3, KernelValue::Mat3(values)) => {
            for (index, value) in values.iter().enumerate() {
                write_f32(
                    bytes,
                    base_offset + portable_abi_lane_offset(abi, index).unwrap() as usize,
                    *value,
                )?;
            }
            Ok(())
        }
        (PortableAbiType::Mat4, KernelValue::Mat4(values)) => {
            for (index, value) in values.iter().enumerate() {
                write_f32(
                    bytes,
                    base_offset + portable_abi_lane_offset(abi, index).unwrap() as usize,
                    *value,
                )?;
            }
            Ok(())
        }
        (PortableAbiType::Array(inner, len), KernelValue::Array(items)) => {
            if items.len() != *len {
                return Err(PortableAbiError::ArrayLengthMismatch {
                    expected: *len,
                    found: items.len(),
                });
            }
            let stride = portable_abi_array_stride(inner) as usize;
            for (index, item) in items.iter().enumerate() {
                write_portable_abi_value_at(inner, item, bytes, base_offset + index * stride)?;
            }
            Ok(())
        }
        (PortableAbiType::Struct { name, fields, .. }, KernelValue::Struct(struct_value)) => {
            if struct_value.name != *name && !struct_value.name.is_empty() {
                return Err(PortableAbiError::TypeMismatch {
                    expected: name.to_string(),
                    found: struct_value.name.to_string(),
                });
            }
            for (index, field) in fields.iter().enumerate() {
                let value = struct_field_value(struct_value, &field.name).ok_or_else(|| {
                    PortableAbiError::MissingField {
                        name: name.clone(),
                        field: field.name.clone(),
                    }
                })?;
                let offset = portable_abi_field_offset(fields, index) as usize;
                write_portable_abi_value_at(&field.ty, value, bytes, base_offset + offset)?;
            }
            Ok(())
        }
        (expected, found) => Err(PortableAbiError::TypeMismatch {
            expected: portable_abi_expected_label(expected),
            found: portable_abi_found_label(found),
        }),
    }
}

fn read_portable_abi_value_at(
    abi: &PortableAbiType,
    bytes: &[u8],
    base_offset: usize,
) -> Result<KernelValue, PortableAbiError> {
    ensure_capacity(abi, bytes, base_offset)?;
    match abi {
        PortableAbiType::Value => Err(PortableAbiError::UnsupportedValueType),
        PortableAbiType::Bool => Ok(KernelValue::Bool(read_u32(bytes, base_offset)? != 0)),
        PortableAbiType::I32 => Ok(KernelValue::I32(read_i32(bytes, base_offset)?)),
        PortableAbiType::U32 => Ok(KernelValue::U32(read_u32(bytes, base_offset)?)),
        PortableAbiType::F32 => Ok(KernelValue::F32(read_f32(bytes, base_offset)?)),
        PortableAbiType::Vec2 => Ok(KernelValue::Vec2([
            read_f32(bytes, base_offset)?,
            read_f32(bytes, base_offset + 4)?,
        ])),
        PortableAbiType::Vec3 => Ok(KernelValue::Vec3([
            read_f32(
                bytes,
                base_offset + portable_abi_lane_offset(abi, 0).unwrap() as usize,
            )?,
            read_f32(
                bytes,
                base_offset + portable_abi_lane_offset(abi, 1).unwrap() as usize,
            )?,
            read_f32(
                bytes,
                base_offset + portable_abi_lane_offset(abi, 2).unwrap() as usize,
            )?,
        ])),
        PortableAbiType::Vec4 => Ok(KernelValue::Vec4([
            read_f32(
                bytes,
                base_offset + portable_abi_lane_offset(abi, 0).unwrap() as usize,
            )?,
            read_f32(
                bytes,
                base_offset + portable_abi_lane_offset(abi, 1).unwrap() as usize,
            )?,
            read_f32(
                bytes,
                base_offset + portable_abi_lane_offset(abi, 2).unwrap() as usize,
            )?,
            read_f32(
                bytes,
                base_offset + portable_abi_lane_offset(abi, 3).unwrap() as usize,
            )?,
        ])),
        PortableAbiType::Quat => Ok(KernelValue::Quat([
            read_f32(
                bytes,
                base_offset + portable_abi_lane_offset(abi, 0).unwrap() as usize,
            )?,
            read_f32(
                bytes,
                base_offset + portable_abi_lane_offset(abi, 1).unwrap() as usize,
            )?,
            read_f32(
                bytes,
                base_offset + portable_abi_lane_offset(abi, 2).unwrap() as usize,
            )?,
            read_f32(
                bytes,
                base_offset + portable_abi_lane_offset(abi, 3).unwrap() as usize,
            )?,
        ])),
        PortableAbiType::Mat3 => {
            let mut values = [0.0f32; 9];
            for (index, value) in values.iter_mut().enumerate() {
                *value = read_f32(
                    bytes,
                    base_offset + portable_abi_lane_offset(abi, index).unwrap() as usize,
                )?;
            }
            Ok(KernelValue::Mat3(values))
        }
        PortableAbiType::Mat4 => {
            let mut values = [0.0f32; 16];
            for (index, value) in values.iter_mut().enumerate() {
                *value = read_f32(
                    bytes,
                    base_offset + portable_abi_lane_offset(abi, index).unwrap() as usize,
                )?;
            }
            Ok(KernelValue::Mat4(values))
        }
        PortableAbiType::Array(inner, len) => {
            let stride = portable_abi_array_stride(inner) as usize;
            let mut items = Vec::with_capacity(*len);
            for index in 0..*len {
                items.push(read_portable_abi_value_at(
                    inner,
                    bytes,
                    base_offset + index * stride,
                )?);
            }
            Ok(KernelValue::Array(items))
        }
        PortableAbiType::Struct { name, fields, .. } => {
            let mut out_fields = Vec::with_capacity(fields.len());
            for (index, field) in fields.iter().enumerate() {
                let offset = portable_abi_field_offset(fields, index) as usize;
                let value = read_portable_abi_value_at(&field.ty, bytes, base_offset + offset)?;
                out_fields.push((field.name.clone(), value));
            }
            Ok(KernelValue::Struct(KernelStructValue {
                name: name.clone(),
                fields: out_fields,
            }))
        }
    }
}

fn struct_field_value<'a>(
    value: &'a KernelStructValue,
    field_name: &SmolStr,
) -> Option<&'a KernelValue> {
    value
        .fields
        .iter()
        .find(|(name, _)| name == field_name)
        .map(|(_, value)| value)
}

fn portable_abi_expected_label(abi: &PortableAbiType) -> String {
    match abi {
        PortableAbiType::Value => "Value".to_string(),
        PortableAbiType::Bool => "Bool".to_string(),
        PortableAbiType::I32 => "I32".to_string(),
        PortableAbiType::U32 => "U32".to_string(),
        PortableAbiType::F32 => "F32".to_string(),
        PortableAbiType::Vec2 => "Vec2".to_string(),
        PortableAbiType::Vec3 => "Vec3".to_string(),
        PortableAbiType::Vec4 => "Vec4".to_string(),
        PortableAbiType::Mat3 => "Mat3".to_string(),
        PortableAbiType::Mat4 => "Mat4".to_string(),
        PortableAbiType::Quat => "Quat".to_string(),
        PortableAbiType::Array(_, len) => format!("Array[{len}]"),
        PortableAbiType::Struct { name, .. } => name.to_string(),
    }
}

fn portable_abi_found_label(value: &KernelValue) -> String {
    match value {
        KernelValue::Nothing => "Nothing".to_string(),
        KernelValue::Bool(_) => "Bool".to_string(),
        KernelValue::I32(_) => "I32".to_string(),
        KernelValue::U32(_) => "U32".to_string(),
        KernelValue::F32(_) => "F32".to_string(),
        KernelValue::Vec2(_) => "Vec2".to_string(),
        KernelValue::Vec3(_) => "Vec3".to_string(),
        KernelValue::Vec4(_) => "Vec4".to_string(),
        KernelValue::Mat3(_) => "Mat3".to_string(),
        KernelValue::Mat4(_) => "Mat4".to_string(),
        KernelValue::Quat(_) => "Quat".to_string(),
        KernelValue::Array(values) => format!("Array[{}]", values.len()),
        KernelValue::Struct(value) => value.name.to_string(),
        KernelValue::Capture(_) => "Capture".to_string(),
        KernelValue::DispatchBackend(_) => "DispatchBackend".to_string(),
        KernelValue::GpuBuffer(_) => "GpuBuffer".to_string(),
        KernelValue::GpuAtomicI32(_) => "GpuAtomicI32".to_string(),
        KernelValue::GpuAtomicU32(_) => "GpuAtomicU32".to_string(),
    }
}

fn ensure_capacity(
    abi: &PortableAbiType,
    bytes: &[u8],
    base_offset: usize,
) -> Result<(), PortableAbiError> {
    let required = base_offset + portable_abi_layout(abi).size as usize;
    if bytes.len() < required {
        return Err(PortableAbiError::BufferTooSmall {
            expected: required,
            found: bytes.len(),
        });
    }
    Ok(())
}

fn write_i32(bytes: &mut [u8], offset: usize, value: i32) -> Result<(), PortableAbiError> {
    write_u32(bytes, offset, value as u32)
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) -> Result<(), PortableAbiError> {
    ensure_raw_capacity(bytes, offset, 4)?;
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_f32(bytes: &mut [u8], offset: usize, value: f32) -> Result<(), PortableAbiError> {
    write_u32(bytes, offset, value.to_bits())
}

fn read_i32(bytes: &[u8], offset: usize) -> Result<i32, PortableAbiError> {
    Ok(read_u32(bytes, offset)? as i32)
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, PortableAbiError> {
    ensure_raw_capacity(bytes, offset, 4)?;
    Ok(u32::from_le_bytes(
        bytes[offset..offset + 4].try_into().unwrap(),
    ))
}

fn read_f32(bytes: &[u8], offset: usize) -> Result<f32, PortableAbiError> {
    Ok(f32::from_bits(read_u32(bytes, offset)?))
}

fn ensure_raw_capacity(bytes: &[u8], offset: usize, width: usize) -> Result<(), PortableAbiError> {
    let required = offset + width;
    if bytes.len() < required {
        return Err(PortableAbiError::BufferTooSmall {
            expected: required,
            found: bytes.len(),
        });
    }
    Ok(())
}
