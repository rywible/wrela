use crate::hir::typeck::Type;
use smol_str::SmolStr;

/// Describes one field inside a GPU buffer layout.
#[derive(Debug, Clone, PartialEq)]
pub struct BufferField {
    pub name: SmolStr,
    pub ty: Type,
    pub offset: usize,
    pub size: usize,
    pub align: usize,
}

/// A computed buffer layout describing byte offsets and total size
/// for a set of typed fields, following std140 alignment rules.
#[derive(Debug, Clone, PartialEq)]
pub struct BufferLayout {
    pub fields: Vec<BufferField>,
    pub stride: usize,
}

/// Return the size in bytes for a GPU type following std140 rules.
pub fn type_size(ty: &Type) -> usize {
    match ty {
        Type::Float | Type::Integer => 4,
        Type::Vec2 => 8,
        Type::Vec3 => 12,
        Type::Vec4 => 16,
        Type::Mat4 => 64, // 4 columns of vec4
        _ => 0,
    }
}

/// Return the alignment in bytes for a GPU type following std140 rules.
pub fn type_align(ty: &Type) -> usize {
    match ty {
        Type::Float | Type::Integer => 4,
        Type::Vec2 => 8,
        // std140: vec3 has alignment of 16
        Type::Vec3 => 16,
        Type::Vec4 => 16,
        Type::Mat4 => 16, // each column is vec4-aligned
        _ => 4,
    }
}

fn align_up(offset: usize, align: usize) -> usize {
    if align == 0 {
        return offset;
    }
    let mask = align - 1;
    (offset + mask) & !mask
}

/// Compute a buffer layout from a list of (name, type) pairs.
/// Uses std140 alignment rules.
pub fn compute_buffer_layout(fields: &[(SmolStr, Type)]) -> BufferLayout {
    let mut result_fields = Vec::new();
    let mut offset = 0usize;

    for (name, ty) in fields {
        let size = type_size(ty);
        let align = type_align(ty);
        offset = align_up(offset, align);

        result_fields.push(BufferField {
            name: name.clone(),
            ty: ty.clone(),
            offset,
            size,
            align,
        });

        offset += size;
    }

    // Final struct alignment: round up to largest alignment.
    let max_align = fields
        .iter()
        .map(|(_, ty)| type_align(ty))
        .max()
        .unwrap_or(4);
    let stride = align_up(offset, max_align);

    BufferLayout {
        fields: result_fields,
        stride,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smol_str::SmolStr;

    #[test]
    fn single_float_field() {
        let fields = vec![(SmolStr::new("x"), Type::Float)];
        let layout = compute_buffer_layout(&fields);
        assert_eq!(layout.fields.len(), 1);
        assert_eq!(layout.fields[0].offset, 0);
        assert_eq!(layout.fields[0].size, 4);
        assert_eq!(layout.stride, 4);
    }

    #[test]
    fn two_floats() {
        let fields = vec![
            (SmolStr::new("a"), Type::Float),
            (SmolStr::new("b"), Type::Float),
        ];
        let layout = compute_buffer_layout(&fields);
        assert_eq!(layout.fields[0].offset, 0);
        assert_eq!(layout.fields[1].offset, 4);
        assert_eq!(layout.stride, 8);
    }

    #[test]
    fn vec3_alignment_is_16() {
        // std140: vec3 has 16-byte alignment
        let fields = vec![
            (SmolStr::new("a"), Type::Float),
            (SmolStr::new("b"), Type::Vec3),
        ];
        let layout = compute_buffer_layout(&fields);
        assert_eq!(layout.fields[0].offset, 0); // float at 0
        assert_eq!(layout.fields[1].offset, 16); // vec3 aligned to 16
        assert_eq!(layout.fields[1].size, 12); // vec3 is 12 bytes
        // stride rounds up to max_align(16)
        assert_eq!(layout.stride, 32);
    }

    #[test]
    fn mat4_layout() {
        let fields = vec![(SmolStr::new("mvp"), Type::Mat4)];
        let layout = compute_buffer_layout(&fields);
        assert_eq!(layout.fields[0].offset, 0);
        assert_eq!(layout.fields[0].size, 64);
        assert_eq!(layout.stride, 64);
    }

    #[test]
    fn mixed_fields_std140() {
        let fields = vec![
            (SmolStr::new("model"), Type::Mat4),
            (SmolStr::new("color"), Type::Vec4),
            (SmolStr::new("intensity"), Type::Float),
        ];
        let layout = compute_buffer_layout(&fields);
        assert_eq!(layout.fields[0].offset, 0); // mat4 at 0
        assert_eq!(layout.fields[0].size, 64);
        assert_eq!(layout.fields[1].offset, 64); // vec4 at 64 (aligned to 16)
        assert_eq!(layout.fields[1].size, 16);
        assert_eq!(layout.fields[2].offset, 80); // float at 80 (aligned to 4)
        assert_eq!(layout.fields[2].size, 4);
        // stride rounds up to max align(16): 84 -> 96
        assert_eq!(layout.stride, 96);
    }

    #[test]
    fn vec2_alignment() {
        let fields = vec![
            (SmolStr::new("a"), Type::Float),
            (SmolStr::new("b"), Type::Vec2),
        ];
        let layout = compute_buffer_layout(&fields);
        assert_eq!(layout.fields[0].offset, 0);
        assert_eq!(layout.fields[1].offset, 8); // vec2 aligned to 8
        assert_eq!(layout.stride, 16); // rounds to max align(8)
    }

    #[test]
    fn empty_layout() {
        let fields: Vec<(SmolStr, Type)> = vec![];
        let layout = compute_buffer_layout(&fields);
        assert_eq!(layout.fields.len(), 0);
        assert_eq!(layout.stride, 0);
    }
}
