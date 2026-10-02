//! Types, and the `GpuData` layout rule.
//!
//! Types live in an interning arena, [`Types`], so a type is a small `Copy` id and two equal types
//! always have the same id: comparing types is comparing ids. Every type is monomorphic: generics
//! were instantiated before the IR, and a struct or enum's name says which instance it is
//! (`Pair<f32>`).

use std::collections::HashMap;
use std::fmt;

use crate::TypeId;

/// A scalar type. All of wrela's tier-0 scalars are here; the GPU has only `bool`, `i32`, `u32`
/// and `f32` (`is_gpu`), and the validator keeps the others out of GPU code.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum Scalar {
    Bool,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    F32,
    F64,
}

impl Scalar {
    pub fn name(self) -> &'static str {
        match self {
            Scalar::Bool => "bool",
            Scalar::I8 => "i8",
            Scalar::U8 => "u8",
            Scalar::I16 => "i16",
            Scalar::U16 => "u16",
            Scalar::I32 => "i32",
            Scalar::U32 => "u32",
            Scalar::I64 => "i64",
            Scalar::U64 => "u64",
            Scalar::F32 => "f32",
            Scalar::F64 => "f64",
        }
    }

    pub fn is_float(self) -> bool {
        matches!(self, Scalar::F32 | Scalar::F64)
    }

    pub fn is_int(self) -> bool {
        !self.is_float() && self != Scalar::Bool
    }

    /// Integers and floats: everything arithmetic applies to.
    pub fn is_numeric(self) -> bool {
        self != Scalar::Bool
    }

    /// Signed integers and floats: what negation applies to.
    pub fn is_signed(self) -> bool {
        matches!(
            self,
            Scalar::I8 | Scalar::I16 | Scalar::I32 | Scalar::I64 | Scalar::F32 | Scalar::F64
        )
    }

    /// The width in bits; `bool` has none.
    pub fn bits(self) -> Option<u32> {
        match self {
            Scalar::Bool => None,
            Scalar::I8 | Scalar::U8 => Some(8),
            Scalar::I16 | Scalar::U16 => Some(16),
            Scalar::I32 | Scalar::U32 | Scalar::F32 => Some(32),
            Scalar::I64 | Scalar::U64 | Scalar::F64 => Some(64),
        }
    }

    /// Whether WGSL has this scalar.
    pub fn is_gpu(self) -> bool {
        matches!(self, Scalar::Bool | Scalar::I32 | Scalar::U32 | Scalar::F32)
    }
}

/// The number of components of a vector, or of columns or rows of a matrix.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum VectorSize {
    Two,
    Three,
    Four,
}

impl VectorSize {
    pub fn count(self) -> u32 {
        match self {
            VectorSize::Two => 2,
            VectorSize::Three => 3,
            VectorSize::Four => 4,
        }
    }

    pub fn from_count(count: u32) -> Option<VectorSize> {
        match count {
            2 => Some(VectorSize::Two),
            3 => Some(VectorSize::Three),
            4 => Some(VectorSize::Four),
            _ => None,
        }
    }
}

/// A type. Structural types (all but structs and enums) are equal when their parts are; structs
/// and enums are nominal, and the validator requires their names to be unique.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Type {
    /// `()`: what a function returns when it returns nothing.
    Unit,
    Scalar(Scalar),
    /// `vec2`…`vec4` hold `f32`, `ivec*` `i32` and `uvec*` `u32`; a vector comparison gives a
    /// vector of `bool`. No other scalar makes a vector.
    Vector {
        size: VectorSize,
        scalar: Scalar,
    },
    /// `mat2`…`mat4` and their non-square forms: `columns` column vectors of `rows` `f32`s,
    /// column-major, as in WGSL.
    Matrix {
        columns: VectorSize,
        rows: VectorSize,
    },
    /// `[T; N]`.
    Array {
        element: TypeId,
        len: u32,
    },
    /// `[T]` and `mut [T]`: a run whose length is known only at run time. It has no value of its
    /// own, so only a parameter or a storage resource has this type, and it's always passed as a
    /// place. On the CPU it's a pointer and a length; on the GPU, a runtime-sized storage array.
    Slice {
        element: TypeId,
    },
    /// `(A, B, …)`, with at least one element.
    Tuple(Vec<TypeId>),
    Struct(StructType),
    Enum(EnumType),
    /// An atomic `i32` or `u32`. It's reached only through a place, with [`crate::Expr::Atomic`].
    Atomic(Scalar),
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct StructType {
    pub name: String,
    pub fields: Vec<StructField>,
    /// The layout of a `GpuData` struct. Both back ends use it, so the bytes in WASM memory are
    /// the bytes the GPU reads and an upload is a copy. `None` leaves the CPU layout to the WASM
    /// back end; such a struct never crosses to the GPU.
    pub gpu_layout: Option<GpuLayout>,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct StructField {
    pub name: String,
    pub ty: TypeId,
}

/// A tagged union. Back ends choose the representation: WGSL has no unions, so the GPU gets a
/// tag and the variants' fields side by side.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct EnumType {
    pub name: String,
    pub variants: Vec<Variant>,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Variant {
    pub name: String,
    pub fields: Vec<TypeId>,
}

/// A `GpuData` struct's layout: its size and alignment, and each field's offset, in bytes.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct GpuLayout {
    pub size: u32,
    pub align: u32,
    pub offsets: Vec<u32>,
}

/// The types of a module. Ids are handed out in order and never change, so iterating gives the
/// same order every time.
#[derive(Clone, Debug, Default)]
pub struct Types {
    list: Vec<Type>,
    ids: HashMap<Type, TypeId>,
}

impl Types {
    pub fn new() -> Self {
        Types::default()
    }

    /// The id of `ty`, adding it if it's new.
    pub fn intern(&mut self, ty: Type) -> TypeId {
        if let Some(&id) = self.ids.get(&ty) {
            return id;
        }
        let id = TypeId(u32::try_from(self.list.len()).unwrap_or(u32::MAX));
        self.list.push(ty.clone());
        self.ids.insert(ty, id);
        id
    }

    pub fn get(&self, id: TypeId) -> Option<&Type> {
        self.list.get(id.index())
    }

    /// The id of `ty` if it's already here.
    pub fn lookup(&self, ty: &Type) -> Option<TypeId> {
        self.ids.get(ty).copied()
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Every type with its id, in id order.
    pub fn iter(&self) -> impl Iterator<Item = (TypeId, &Type)> {
        self.list
            .iter()
            .enumerate()
            .map(|(i, ty)| (TypeId(u32::try_from(i).unwrap_or(u32::MAX)), ty))
    }

    pub fn unit(&mut self) -> TypeId {
        self.intern(Type::Unit)
    }

    pub fn scalar(&mut self, scalar: Scalar) -> TypeId {
        self.intern(Type::Scalar(scalar))
    }

    pub fn vector(&mut self, size: VectorSize, scalar: Scalar) -> TypeId {
        self.intern(Type::Vector { size, scalar })
    }

    /// Adds a struct. With `gpu_data`, its [`GpuLayout`] is computed from the fields, which must
    /// all be able to cross to the GPU.
    pub fn add_struct(
        &mut self,
        name: impl Into<String>,
        fields: Vec<StructField>,
        gpu_data: bool,
    ) -> Result<TypeId, LayoutError> {
        let gpu_layout = if gpu_data {
            let types: Vec<TypeId> = fields.iter().map(|f| f.ty).collect();
            Some(self.gpu_struct_layout(&types)?)
        } else {
            None
        };
        Ok(self.intern(Type::Struct(StructType {
            name: name.into(),
            fields,
            gpu_layout,
        })))
    }

    /// A type's name as the printer writes it: `f32`, `vec3<f32>`, `[u32; 4]`, `Scene`.
    pub fn name(&self, id: TypeId) -> TypeName<'_> {
        TypeName { types: self, id }
    }

    /// The size and alignment of `ty` under the `GpuData` rule (see [`Types::gpu_struct_layout`]).
    pub fn gpu_size_align(&self, ty: TypeId) -> Result<(u32, u32), LayoutError> {
        self.size_align(ty, 0)
    }

    /// The `GpuData` layout of a struct with these field types, in order.
    ///
    /// It's WGSL's host-shareable layout, adjusted so it's also valid in the uniform address
    /// space: a struct or array member is aligned to 16 bytes and the next member starts at
    /// least a multiple of 16 bytes after it, and array strides are rounded up to 16. A value has
    /// this one layout wherever it is, in uniform or storage buffers and in WASM memory.
    pub fn gpu_struct_layout(&self, fields: &[TypeId]) -> Result<GpuLayout, LayoutError> {
        self.struct_layout(fields, 0)
    }

    fn struct_layout(&self, fields: &[TypeId], depth: u32) -> Result<GpuLayout, LayoutError> {
        if fields.is_empty() {
            return Err(LayoutError::NoFields);
        }
        let mut offsets = Vec::with_capacity(fields.len());
        let mut cursor = 0u32;
        let mut align = 4u32;
        for &field in fields {
            let (size, field_align) = self.size_align(field, depth + 1)?;
            // Structs and arrays inside a uniform struct need 16-byte alignment and padding.
            let aggregate = matches!(self.get(field), Some(Type::Struct(_) | Type::Array { .. }));
            let (field_align, size) = if aggregate {
                (field_align.max(16), round_up(16, size)?)
            } else {
                (field_align, size)
            };
            let offset = round_up(field_align, cursor)?;
            offsets.push(offset);
            cursor = offset.checked_add(size).ok_or(LayoutError::TooLarge)?;
            align = align.max(field_align);
        }
        Ok(GpuLayout {
            size: round_up(align, cursor)?,
            align,
            offsets,
        })
    }

    fn size_align(&self, ty: TypeId, depth: u32) -> Result<(u32, u32), LayoutError> {
        if depth > MAX_DEPTH {
            return Err(LayoutError::TooDeep);
        }
        let not_shareable = Err(LayoutError::NotHostShareable(ty));
        match self.get(ty).ok_or(LayoutError::UnknownType(ty))? {
            Type::Scalar(Scalar::I32 | Scalar::U32 | Scalar::F32)
            | Type::Atomic(Scalar::I32 | Scalar::U32) => Ok((4, 4)),
            Type::Vector {
                size,
                scalar: Scalar::I32 | Scalar::U32 | Scalar::F32,
            } => Ok(vector_size_align(*size)),
            Type::Matrix { columns, rows } => {
                let (size, align) = vector_size_align(*rows);
                let stride = round_up(align, size)?;
                Ok((stride * columns.count(), align))
            }
            Type::Array { element, len } => {
                if *len == 0 {
                    return Err(LayoutError::ZeroLength(ty));
                }
                let (size, align) = self.size_align(*element, depth + 1)?;
                let stride = round_up(16, round_up(align, size)?)?;
                let size = stride.checked_mul(*len).ok_or(LayoutError::TooLarge)?;
                Ok((size, align.max(16)))
            }
            Type::Struct(s) => match &s.gpu_layout {
                Some(layout) => Ok((layout.size, layout.align)),
                None => not_shareable,
            },
            _ => not_shareable,
        }
    }
}

/// How deep types may nest before the layout rule gives up: far beyond real programs, and it
/// keeps a malformed, cyclic arena from recursing forever.
const MAX_DEPTH: u32 = 64;

fn vector_size_align(size: VectorSize) -> (u32, u32) {
    match size {
        VectorSize::Two => (8, 8),
        VectorSize::Three => (12, 16),
        VectorSize::Four => (16, 16),
    }
}

fn round_up(align: u32, n: u32) -> Result<u32, LayoutError> {
    n.checked_next_multiple_of(align)
        .ok_or(LayoutError::TooLarge)
}

/// Why a type can't cross to the GPU.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LayoutError {
    /// `bool`, a 64-, 16- or 8-bit scalar, a slice, tuple, enum, unit, or a struct that isn't
    /// `GpuData`.
    NotHostShareable(TypeId),
    /// WGSL has no empty structs.
    NoFields,
    /// WGSL has no zero-length arrays.
    ZeroLength(TypeId),
    /// More than 4 GiB.
    TooLarge,
    TooDeep,
    UnknownType(TypeId),
}

impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LayoutError::NotHostShareable(ty) => {
                write!(f, "type {ty:?} can't be part of GPU data")
            }
            LayoutError::NoFields => f.write_str("GPU data needs at least one field"),
            LayoutError::ZeroLength(ty) => write!(f, "type {ty:?} is an empty array"),
            LayoutError::TooLarge => f.write_str("the layout is larger than 4 GiB"),
            LayoutError::TooDeep => f.write_str("the types nest too deeply"),
            LayoutError::UnknownType(ty) => write!(f, "type {ty:?} doesn't exist"),
        }
    }
}

impl std::error::Error for LayoutError {}

/// Displays a type by name; see [`Types::name`]. An id with no type prints as `?N`.
#[derive(Clone, Copy, Debug)]
pub struct TypeName<'a> {
    types: &'a Types,
    id: TypeId,
}

impl fmt::Display for TypeName<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_type(f, self.types, self.id, 0)
    }
}

fn write_type(f: &mut fmt::Formatter<'_>, types: &Types, id: TypeId, depth: u32) -> fmt::Result {
    let Some(ty) = types.get(id).filter(|_| depth <= MAX_DEPTH) else {
        return write!(f, "?{}", id.0);
    };
    let sub = |f: &mut fmt::Formatter<'_>, id| write_type(f, types, id, depth + 1);
    match ty {
        Type::Unit => f.write_str("()"),
        Type::Scalar(s) => f.write_str(s.name()),
        Type::Vector { size, scalar } => write!(f, "vec{}<{}>", size.count(), scalar.name()),
        Type::Matrix { columns, rows } => {
            write!(f, "mat{}x{}<f32>", columns.count(), rows.count())
        }
        Type::Array { element, len } => {
            f.write_str("[")?;
            sub(f, *element)?;
            write!(f, "; {len}]")
        }
        Type::Slice { element } => {
            f.write_str("[")?;
            sub(f, *element)?;
            f.write_str("]")
        }
        Type::Tuple(elements) => {
            f.write_str("(")?;
            for (i, &element) in elements.iter().enumerate() {
                if i > 0 {
                    f.write_str(", ")?;
                }
                sub(f, element)?;
            }
            if elements.len() == 1 {
                f.write_str(",")?;
            }
            f.write_str(")")
        }
        Type::Struct(s) => f.write_str(&s.name),
        Type::Enum(e) => f.write_str(&e.name),
        Type::Atomic(s) => write!(f, "atomic<{}>", s.name()),
    }
}
