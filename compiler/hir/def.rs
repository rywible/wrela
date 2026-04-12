use crate::execution_policy::{RequiredGuaranteeClass, SelectedMethodClass};
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
    Pure,
    Kernel,
    System,
    Field,
    Region,
    Domain,
    Render,
    View,
    Radiance,
    Volume,
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
pub enum RegionDetailLevel {
    Coarse,
    Fine,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegionLayerBinding {
    pub detail: RegionDetailLevel,
    pub shape: SmolStr,
    pub shape_span: Option<TextRange>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegionMetadata {
    pub layers: Vec<RegionLayerBinding>,
    pub items: Vec<RegionItemMetadata>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionComposeKind {
    Place,
    Overlay,
    Replace,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RegionItemMetadata {
    Compose {
        kind: RegionComposeKind,
        name: SmolStr,
        name_span: Option<TextRange>,
        shape: SmolStr,
        shape_span: Option<TextRange>,
        detail: Option<RegionDetailLevel>,
    },
    Scatter {
        name: SmolStr,
        name_span: Option<TextRange>,
        items: Vec<RegionItemMetadata>,
    },
    Conditional {
        condition: Body,
        then_items: Vec<RegionItemMetadata>,
        else_items: Vec<RegionItemMetadata>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainGeometryDetail {
    Coarse,
    Fine,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DomainMetadata {
    pub geometry_detail: DomainGeometryDetail,
    pub material: bool,
    pub radiance: bool,
    pub media: bool,
    pub execution_policy: Option<DomainExecutionPolicyMetadata>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DomainExecutionPolicyMetadata {
    pub required_guarantee: RequiredGuaranteeClass,
    pub selected_method: SelectedMethodClass,
    pub max_distance: Option<Body>,
    pub min_step: Option<Body>,
    pub hit_epsilon: Option<Body>,
    pub max_steps: Option<Body>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PresentationMetadata {
    pub view: PresentationViewMetadata,
    pub frame: PresentationFrameMetadata,
    pub lighting: PresentationLightingMetadata,
    pub compatibility: PresentationCompatibilityProjectionMetadata,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PresentationViewMetadata {
    pub projection: PresentationProjectionMetadata,
    pub width: Option<Body>,
    pub height: Option<Body>,
    pub viewport: Option<Body>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresentationProjectionMetadata {
    pub source: PresentationProjectionSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentationProjectionSource {
    CameraVerticalFovDegrees,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PresentationFrameMetadata {
    pub domain: Option<Body>,
    pub quality: Option<Body>,
    pub outputs: Option<Body>,
    pub history: Option<Body>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PresentationLightingMetadata {
    pub light: Option<Body>,
    pub lights: Option<Body>,
    pub fill_dir: Option<Body>,
    pub fill_strength: Option<Body>,
    pub ambient_color: Option<Body>,
    pub grouped: Option<Body>,
    pub light_compatibility_alias: bool,
    pub fill_dir_compatibility_alias: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PresentationCompatibilityProjectionMetadata {
    /// Compatibility-only projection override retained for current preview
    /// stability. Canonical projection is represented by
    /// `PresentationProjectionSource::CameraVerticalFovDegrees`.
    pub world_up: Option<Body>,
    /// Compatibility-only projection scale retained for current preview
    /// stability. New presentation contracts should treat the camera FOV as
    /// canonical and report this as a legacy projection input.
    pub view_scale: Option<Body>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldPrimitive {
    Sphere,
    Box,
    Capsule,
    Cylinder,
    Plane,
    Torus,
    RoundedBox,
    Ellipsoid,
    Cone,
    CappedCone,
    BoxFrame,
    Slab,
    TrianglePrism,
    HexPrism,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfilePrimitive {
    Circle2,
    Rect2,
    RoundedRect2,
    Capsule2,
    Segment2,
    Polygon2,
    Polyline2,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProfileExpr {
    Primitive {
        primitive: ProfilePrimitive,
        args: Vec<Arg>,
    },
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
    pub smooth_op_count: u32,
    pub deform_op_count: u32,
}

impl GraphTraceMetadata {
    pub const fn pessimistic() -> Self {
        Self {
            class: FieldClass::Conservative,
            support: FieldSupport::Unknown,
            bounds: FieldBounds::Unknown,
            can_coarse_support_pruning: false,
            smooth_op_count: 0,
            deform_op_count: 0,
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
            smooth_op_count: 0,
            deform_op_count: 0,
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
            smooth_op_count: 0,
            deform_op_count: 0,
        }
    }

    pub const fn combine_class(self, other: Self) -> FieldClass {
        match (self.class, other.class) {
            (FieldClass::Exact, FieldClass::Exact) => FieldClass::Exact,
            _ => FieldClass::Conservative,
        }
    }

    pub const fn with_march_cost(mut self, smooth_op_count: u32, deform_op_count: u32) -> Self {
        self.smooth_op_count = smooth_op_count;
        self.deform_op_count = deform_op_count;
        self
    }

    pub const fn add_march_cost(mut self, smooth_op_count: u32, deform_op_count: u32) -> Self {
        self.smooth_op_count += smooth_op_count;
        self.deform_op_count += deform_op_count;
        self
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
    Translate {
        translate: Body,
        body: Box<FieldExpr>,
    },
    Rotate {
        rotate: Body,
        body: Box<FieldExpr>,
    },
    UniformScale {
        scale: Body,
        body: Box<FieldExpr>,
    },
    AffineTransform {
        transform: Body,
        body: Box<FieldExpr>,
    },
    Warp {
        warp: Body,
        body: Box<FieldExpr>,
    },
    RepeatLinear {
        repeat: Body,
        body: Box<FieldExpr>,
    },
    RepeatGrid {
        repeat: Body,
        body: Box<FieldExpr>,
    },
    RadialRepeat {
        radial: Body,
        body: Box<FieldExpr>,
    },
    MirrorArray {
        mirror: Body,
        body: Box<FieldExpr>,
    },
    InstanceArray {
        instance: Body,
        body: Box<FieldExpr>,
    },
    SmoothUnion {
        smoothing: Body,
        items: Vec<FieldExpr>,
    },
    SmoothIntersection {
        smoothing: Body,
        items: Vec<FieldExpr>,
    },
    SmoothSubtract {
        smoothing: Body,
        left: Box<FieldExpr>,
        right: Box<FieldExpr>,
    },
    Bend {
        bend: Body,
        body: Box<FieldExpr>,
    },
    Twist {
        twist: Body,
        body: Box<FieldExpr>,
    },
    Taper {
        taper: Body,
        body: Box<FieldExpr>,
    },
    Displace {
        displace: Body,
        body: Box<FieldExpr>,
    },
    Extrude {
        height: Body,
        profile: ProfileExpr,
    },
    Revolve {
        profile: ProfileExpr,
    },
    Sweep {
        path: Body,
        profile: ProfileExpr,
    },
    Loft {
        height: Body,
        from: ProfileExpr,
        to: ProfileExpr,
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
    pub radiance: Option<SmolStr>,
    pub volume: Option<SmolStr>,
    pub payload: Body,
    pub feature_id: u32,
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
    pub region: Option<RegionMetadata>,
    pub domain: Option<DomainMetadata>,
    pub presentation: Option<PresentationMetadata>,
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
            FunctionRole::Function
            | FunctionRole::System
            | FunctionRole::Region
            | FunctionRole::Domain
            | FunctionRole::Render
            | FunctionRole::View => FunctionLane::Host,
            FunctionRole::Pure
            | FunctionRole::Kernel
            | FunctionRole::Field
            | FunctionRole::Radiance
            | FunctionRole::Volume
            | FunctionRole::Shape
            | FunctionRole::Material => FunctionLane::Portable,
        }
    }

    pub fn visit_analysis_bodies<'a, F>(&'a self, mut visit: F)
    where
        F: FnMut(&'a Body),
    {
        let mut seen = Vec::new();
        if let Some(body) = &self.body {
            visit_body_once(body, &mut seen, &mut visit);
        }
        if let Some(field) = &self.field {
            if let Some(support) = &field.authored_support {
                visit_body_once(support, &mut seen, &mut visit);
            }
            if let Some(bounds) = &field.authored_bounds {
                visit_body_once(bounds, &mut seen, &mut visit);
            }
        }
        if let Some(graph) = &self.field_graph {
            visit_field_expr_bodies(&graph.root, &mut seen, &mut visit);
        }
    }
}

pub fn body_key(body: &Body) -> usize {
    body as *const Body as usize
}

fn visit_body_once<'a, F>(body: &'a Body, seen: &mut Vec<&'a Body>, visit: &mut F)
where
    F: FnMut(&'a Body),
{
    if seen.iter().any(|existing| *existing == body) {
        return;
    }
    seen.push(body);
    visit(body);
}

fn visit_field_expr_bodies<'a, F>(expr: &'a FieldExpr, seen: &mut Vec<&'a Body>, visit: &mut F)
where
    F: FnMut(&'a Body),
{
    match expr {
        FieldExpr::Use { .. } | FieldExpr::Primitive { .. } => {}
        FieldExpr::Union { items } | FieldExpr::Intersection { items } => {
            for item in items {
                visit_field_expr_bodies(item, seen, visit);
            }
        }
        FieldExpr::Subtract { left, right } => {
            visit_field_expr_bodies(left, seen, visit);
            visit_field_expr_bodies(right, seen, visit);
        }
        FieldExpr::Translate { translate, body } => {
            visit_body_once(translate, seen, visit);
            visit_field_expr_bodies(body, seen, visit);
        }
        FieldExpr::Rotate { rotate, body } => {
            visit_body_once(rotate, seen, visit);
            visit_field_expr_bodies(body, seen, visit);
        }
        FieldExpr::UniformScale { scale, body } => {
            visit_body_once(scale, seen, visit);
            visit_field_expr_bodies(body, seen, visit);
        }
        FieldExpr::AffineTransform { transform, body } => {
            visit_body_once(transform, seen, visit);
            visit_field_expr_bodies(body, seen, visit);
        }
        FieldExpr::Warp { warp, body } => {
            visit_body_once(warp, seen, visit);
            visit_field_expr_bodies(body, seen, visit);
        }
        FieldExpr::RepeatLinear { repeat, body } => {
            visit_body_once(repeat, seen, visit);
            visit_field_expr_bodies(body, seen, visit);
        }
        FieldExpr::RepeatGrid { repeat, body } => {
            visit_body_once(repeat, seen, visit);
            visit_field_expr_bodies(body, seen, visit);
        }
        FieldExpr::RadialRepeat { radial, body } => {
            visit_body_once(radial, seen, visit);
            visit_field_expr_bodies(body, seen, visit);
        }
        FieldExpr::MirrorArray { mirror, body } => {
            visit_body_once(mirror, seen, visit);
            visit_field_expr_bodies(body, seen, visit);
        }
        FieldExpr::InstanceArray { instance, body } => {
            visit_body_once(instance, seen, visit);
            visit_field_expr_bodies(body, seen, visit);
        }
        FieldExpr::SmoothUnion { smoothing, items }
        | FieldExpr::SmoothIntersection { smoothing, items } => {
            visit_body_once(smoothing, seen, visit);
            for item in items {
                visit_field_expr_bodies(item, seen, visit);
            }
        }
        FieldExpr::SmoothSubtract {
            smoothing,
            left,
            right,
        } => {
            visit_body_once(smoothing, seen, visit);
            visit_field_expr_bodies(left, seen, visit);
            visit_field_expr_bodies(right, seen, visit);
        }
        FieldExpr::Bend { bend, body } => {
            visit_body_once(bend, seen, visit);
            visit_field_expr_bodies(body, seen, visit);
        }
        FieldExpr::Twist { twist, body } => {
            visit_body_once(twist, seen, visit);
            visit_field_expr_bodies(body, seen, visit);
        }
        FieldExpr::Taper { taper, body } => {
            visit_body_once(taper, seen, visit);
            visit_field_expr_bodies(body, seen, visit);
        }
        FieldExpr::Displace { displace, body } => {
            visit_body_once(displace, seen, visit);
            visit_field_expr_bodies(body, seen, visit);
        }
        FieldExpr::Extrude { height, profile } => {
            visit_body_once(height, seen, visit);
            visit_profile_expr_bodies(profile, seen, visit);
        }
        FieldExpr::Revolve { profile } => {
            visit_profile_expr_bodies(profile, seen, visit);
        }
        FieldExpr::Sweep { path, profile } => {
            visit_body_once(path, seen, visit);
            visit_profile_expr_bodies(profile, seen, visit);
        }
        FieldExpr::Loft { height, from, to } => {
            visit_body_once(height, seen, visit);
            visit_profile_expr_bodies(from, seen, visit);
            visit_profile_expr_bodies(to, seen, visit);
        }
        FieldExpr::Custom { body } => {
            visit_body_once(body, seen, visit);
        }
    }
}

fn visit_profile_expr_bodies<'a, F>(
    profile: &'a ProfileExpr,
    _seen: &mut Vec<&'a Body>,
    _visit: &mut F,
) where
    F: FnMut(&'a Body),
{
    match profile {
        ProfileExpr::Primitive { .. } => {}
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
