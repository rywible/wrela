#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicalLayoutStrategy {
    DenseBuffer,
    RowAligned { row_alignment: u32 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct PhysicalLayoutPlan {
    pub strategy: PhysicalLayoutStrategy,
    pub width: u32,
    pub height: u32,
    pub element_count: u32,
    pub element_stride: u32,
    pub row_stride: u32,
    pub total_size: u32,
}

impl PhysicalLayoutPlan {
    pub fn dense_buffer(width: u32, height: u32, element_stride: u32) -> Self {
        let element_count = width.saturating_mul(height);
        let row_stride = width.saturating_mul(element_stride);
        let total_size = row_stride.saturating_mul(height);
        Self {
            strategy: PhysicalLayoutStrategy::DenseBuffer,
            width,
            height,
            element_count,
            element_stride,
            row_stride,
            total_size,
        }
    }

    pub fn row_aligned(width: u32, height: u32, element_stride: u32, row_alignment: u32) -> Self {
        let element_count = width.saturating_mul(height);
        let row_stride = align_up(width.saturating_mul(element_stride), row_alignment.max(1));
        let total_size = row_stride.saturating_mul(height);
        Self {
            strategy: PhysicalLayoutStrategy::RowAligned {
                row_alignment: row_alignment.max(1),
            },
            width,
            height,
            element_count,
            element_stride,
            row_stride,
            total_size,
        }
    }
}

fn align_up(value: u32, alignment: u32) -> u32 {
    let alignment = alignment.max(1);
    let remainder = value % alignment;
    if remainder == 0 {
        value
    } else {
        value.saturating_add(alignment - remainder)
    }
}
