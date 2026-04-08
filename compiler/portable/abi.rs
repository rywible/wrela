use smol_str::SmolStr;

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
