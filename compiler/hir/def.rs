use crate::hir::arena::{Arena, Idx};
use crate::hir::body::{Arg, Body, Literal, UseName};
use miette::SourceSpan;
use rowan::TextRange;
use smol_str::SmolStr;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub functions: Arena<Function>,
    pub classes: Arena<Class>,
    pub enums: Arena<Enum>,
    pub interfaces: Arena<Interface>,
    pub shapes: Arena<Shape>,
    pub uses: Vec<UseStmt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceOrigin {
    pub path: PathBuf,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Private,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionKind {
    Function,
    Method,
    Derived,
    Check,
    CheckMethod,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeParam {
    pub name: SmolStr,
    pub bounds: Vec<SmolStr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassRole {
    Class,
    Resource,
    Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionRole {
    Function,
    Kernel,
    System,
    Field,
    Shape,
    Material,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldClass {
    Exact,
    Conservative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Distance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldSupport {
    Unknown,
    Bounded,
    Periodic,
    Unbounded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldBounds {
    Unknown,
    Bounded,
    Unbounded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldPrimitive {
    Sphere,
    Box,
    Capsule,
    Cylinder,
    Plane,
    Torus,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldMetadata {
    pub class: FieldClass,
    pub kind: FieldKind,
    pub support: FieldSupport,
    pub bounds: FieldBounds,
    pub trace: GraphTraceMetadata,
    pub authored_support: Option<Body>,
    pub authored_bounds: Option<Body>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphTraceMetadata {
    pub class: FieldClass,
    pub support: FieldSupport,
    pub bounds: FieldBounds,
    pub can_coarse_support_pruning: bool,
}

impl GraphTraceMetadata {
    pub const fn pessimistic() -> Self {
        Self {
            class: FieldClass::Conservative,
            support: FieldSupport::Unknown,
            bounds: FieldBounds::Unknown,
            can_coarse_support_pruning: false,
        }
    }

    pub const fn exact(
        support: FieldSupport,
        bounds: FieldBounds,
        can_coarse_support_pruning: bool,
    ) -> Self {
        Self {
            class: FieldClass::Exact,
            support,
            bounds,
            can_coarse_support_pruning,
        }
    }

    pub const fn conservative(
        support: FieldSupport,
        bounds: FieldBounds,
        can_coarse_support_pruning: bool,
    ) -> Self {
        Self {
            class: FieldClass::Conservative,
            support,
            bounds,
            can_coarse_support_pruning,
        }
    }

    pub const fn combine_class(self, other: Self) -> FieldClass {
        match (self.class, other.class) {
            (FieldClass::Exact, FieldClass::Exact) => FieldClass::Exact,
            _ => FieldClass::Conservative,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldGraph {
    pub root: FieldExpr,
    pub trace: GraphTraceMetadata,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldExpr {
    Use {
        target: SmolStr,
    },
    Primitive {
        primitive: FieldPrimitive,
        args: Vec<Arg>,
    },
    Union {
        items: Vec<FieldExpr>,
    },
    Intersection {
        items: Vec<FieldExpr>,
    },
    Subtract {
        left: Box<FieldExpr>,
        right: Box<FieldExpr>,
    },
    Transform {
        transform: Body,
        body: Box<FieldExpr>,
    },
    Mirror {
        mirror: Body,
        body: Box<FieldExpr>,
    },
    Repeat {
        repeat: Body,
        body: Box<FieldExpr>,
    },
    Instance {
        instance: Body,
        body: Box<FieldExpr>,
    },
    Custom {
        body: Body,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShapeGraph {
    pub root: ShapeExpr,
    pub provenance: Option<ShapeProvenanceExpr>,
    pub trace: GraphTraceMetadata,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShapeLeaf {
    pub field: SmolStr,
    pub material: SmolStr,
    pub payload: Body,
    pub feature_id: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeMergeProvenancePolicy {
    Nearest,
    Ordered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeSubtractProvenancePolicy {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ShapeExpr {
    Use {
        target: SmolStr,
    },
    Union {
        items: Vec<ShapeExpr>,
    },
    Intersection {
        items: Vec<ShapeExpr>,
    },
    Subtract {
        left: Box<ShapeExpr>,
        right: Box<ShapeExpr>,
    },
    Leaf(ShapeLeaf),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ShapeProvenanceExpr {
    Use {
        target: SmolStr,
    },
    Union {
        provenance: ShapeMergeProvenancePolicy,
        items: Vec<ShapeProvenanceExpr>,
    },
    Intersection {
        provenance: ShapeMergeProvenancePolicy,
        items: Vec<ShapeProvenanceExpr>,
    },
    Subtract {
        provenance: ShapeSubtractProvenancePolicy,
        left: Box<ShapeProvenanceExpr>,
        right: Box<ShapeProvenanceExpr>,
    },
    Leaf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionLane {
    Host,
    Portable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemMetadata {
    pub stage: Option<SmolStr>,
    pub reads: Vec<SmolStr>,
    pub writes: Vec<SmolStr>,
    pub before: Vec<SmolStr>,
    pub after: Vec<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeArg {
    pub key: SmolStr,
    pub key_span: Option<TextRange>,
    pub value: SmolStr,
    pub value_span: Option<TextRange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeAnnotation {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub args: Vec<AttributeArg>,
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub attributes: Vec<AttributeAnnotation>,
    pub visibility: Visibility,
    pub kind: FunctionKind,
    pub role: FunctionRole,
    pub field: Option<FieldMetadata>,
    pub field_graph: Option<FieldGraph>,
    pub system_metadata: Option<SystemMetadata>,
    pub type_params: Vec<TypeParam>,
    pub params: Vec<Param>,
    pub ret_type: Option<TypeRef>,
    pub body: Option<Body>,
}

impl Function {
    pub fn lane(&self) -> FunctionLane {
        match self.role {
            FunctionRole::Function | FunctionRole::System => FunctionLane::Host,
            FunctionRole::Kernel
            | FunctionRole::Field
            | FunctionRole::Shape
            | FunctionRole::Material => FunctionLane::Portable,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub visibility: Visibility,
    pub graph: Option<ShapeGraph>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Class {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub visibility: Visibility,
    pub role: ClassRole,
    pub type_params: Vec<TypeParam>,
    pub fields: Vec<Field>,
    pub methods: Vec<Idx<Function>>,
    pub implements: Vec<SmolStr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Enum {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub visibility: Visibility,
    pub type_params: Vec<TypeParam>,
    pub variants: Vec<EnumVariant>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariant {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub params: Vec<Param>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Interface {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub visibility: Visibility,
    pub type_params: Vec<TypeParam>,
    pub methods: Vec<InterfaceMethod>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceMethod {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub params: Vec<Param>,
    pub ret_type: Option<TypeRef>,
    pub kind: InterfaceMethodKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterfaceMethodKind {
    Method,
    Check,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UseStmt {
    pub names: Vec<UseName>,
    pub module: SmolStr,
    pub module_span: Option<TextRange>,
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub ty: Option<TypeRef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub visibility: Visibility,
    pub ty: Option<TypeRef>,
    pub mutable: bool,
    pub default: Option<FieldDefault>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldDefault {
    Literal(Literal),
    List(Vec<FieldDefault>),
    Map(Vec<(FieldDefault, FieldDefault)>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeRef {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub args: Vec<TypeRef>,
}
