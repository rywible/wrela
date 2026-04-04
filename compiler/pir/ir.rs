use crate::hir::{BinaryOp, UnaryOp};
use rowan::TextRange;
use smol_str::SmolStr;

#[derive(Debug, Clone, PartialEq)]
pub struct PirModule {
    pub entry: SmolStr,
    pub functions: Vec<PirFunction>,
}

impl PirModule {
    pub fn function(&self, name: &str) -> Option<&PirFunction> {
        self.functions.iter().find(|func| func.name == name)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PirFunction {
    pub name: SmolStr,
    pub params: Vec<PirParam>,
    pub ret: PirType,
    pub body: PirBlock,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PirParam {
    pub name: SmolStr,
    pub ty: PirType,
}

pub type PirBlock = Vec<PirStmt>;

#[derive(Debug, Clone, PartialEq)]
pub enum PirStmt {
    Let {
        name: SmolStr,
        mutable: bool,
        ty: PirType,
        value: PirExpr,
        span: TextRange,
    },
    Assign {
        name: SmolStr,
        value: PirExpr,
        span: TextRange,
    },
    Expr {
        value: PirExpr,
        span: TextRange,
    },
    If {
        condition: PirExpr,
        then_block: PirBlock,
        else_block: PirBlock,
        span: TextRange,
    },
    Return {
        value: Option<PirExpr>,
        span: TextRange,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum PirExpr {
    Literal(PirValue),
    Var {
        name: SmolStr,
        ty: PirType,
    },
    Unary {
        op: UnaryOp,
        expr: Box<PirExpr>,
        ty: PirType,
    },
    Binary {
        op: BinaryOp,
        lhs: Box<PirExpr>,
        rhs: Box<PirExpr>,
        ty: PirType,
    },
    Call {
        target: PirCallTarget,
        args: Vec<PirExpr>,
        ty: PirType,
    },
    Member {
        base: Box<PirExpr>,
        member: SmolStr,
        ty: PirType,
    },
    Index {
        base: Box<PirExpr>,
        index: Box<PirExpr>,
        ty: PirType,
    },
    ArrayLiteral {
        items: Vec<PirExpr>,
        ty: PirType,
    },
    StructLiteral {
        name: SmolStr,
        fields: Vec<(SmolStr, PirExpr)>,
        ty: PirType,
    },
}

impl PirExpr {
    pub fn ty(&self) -> &PirType {
        match self {
            PirExpr::Literal(value) => value.ty(),
            PirExpr::Var { ty, .. }
            | PirExpr::Unary { ty, .. }
            | PirExpr::Binary { ty, .. }
            | PirExpr::Call { ty, .. }
            | PirExpr::Member { ty, .. }
            | PirExpr::Index { ty, .. }
            | PirExpr::ArrayLiteral { ty, .. }
            | PirExpr::StructLiteral { ty, .. } => ty,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PirCallTarget {
    Function(SmolStr),
    Intrinsic(PirIntrinsic),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PirIntrinsic {
    CastI32,
    CastU32,
    CastI64,
    CastU64,
    CastF32,
    Vec2,
    Vec3,
    Vec4,
    Quat,
    Mat3Identity,
    Mat3Cols,
    Mat4Identity,
    Mat4Cols,
    Bounds2Center,
    Bounds2Size,
    Bounds3Center,
    Bounds3Size,
    Transform3Identity,
    TransformPoint,
    TransformVector,
    TransformNormal,
    ComposeTransform3,
    InverseTransform3,
    FieldTransformPoint,
    FieldInstancePoint,
    FieldMirrorPoint,
    FieldRepeatPoint,
    Sphere,
    Box,
    Capsule,
    Cylinder,
    Plane,
    Torus,
    FieldUnion,
    FieldIntersection,
    FieldSubtract,
    Dot,
    Length,
    Normalize,
    Cross,
    Min,
    Max,
    Clamp,
    Mix,
    Abs,
    Sign,
    Floor,
    Ceil,
    Fract,
    Sin,
    Cos,
    Sqrt,
    Pow,
    Distance,
    Reflect,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PirType {
    Nothing,
    Bool,
    I32,
    U32,
    I64,
    U64,
    F32,
    Vec2,
    Vec3,
    Vec4,
    Mat3,
    Mat4,
    Quat,
    Array(Box<PirType>, usize),
    Struct(PirStructType),
}

impl PirType {
    pub fn field(&self, name: &str) -> Option<&PirStructField> {
        match self {
            PirType::Struct(layout) => layout.fields.iter().find(|field| field.name == name),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PirStructType {
    pub name: SmolStr,
    pub fields: Vec<PirStructField>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PirStructField {
    pub name: SmolStr,
    pub ty: PirType,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PirValue {
    Nothing,
    Bool(bool),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    F32(f32),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
    Mat3([f32; 9]),
    Mat4([f32; 16]),
    Quat([f32; 4]),
    Array(Vec<PirValue>),
    Struct(PirStructValue),
}

impl PirValue {
    pub fn ty(&self) -> &PirType {
        match self {
            PirValue::Nothing => &PirType::Nothing,
            PirValue::Bool(_) => &PirType::Bool,
            PirValue::I32(_) => &PirType::I32,
            PirValue::U32(_) => &PirType::U32,
            PirValue::I64(_) => &PirType::I64,
            PirValue::U64(_) => &PirType::U64,
            PirValue::F32(_) => &PirType::F32,
            PirValue::Vec2(_) => &PirType::Vec2,
            PirValue::Vec3(_) => &PirType::Vec3,
            PirValue::Vec4(_) => &PirType::Vec4,
            PirValue::Mat3(_) => &PirType::Mat3,
            PirValue::Mat4(_) => &PirType::Mat4,
            PirValue::Quat(_) => &PirType::Quat,
            PirValue::Array(items) => {
                static NOTHING: PirType = PirType::Nothing;
                items.first().map(PirValue::ty).unwrap_or(&NOTHING)
            }
            PirValue::Struct(value) => &value.ty,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PirStructValue {
    pub ty: PirType,
    pub fields: Vec<(SmolStr, PirValue)>,
}

impl PirStructValue {
    pub fn field(&self, name: &str) -> Option<&PirValue> {
        self.fields
            .iter()
            .find(|(field_name, _)| field_name == name)
            .map(|(_, value)| value)
    }
}
