use crate::hir;
use smol_str::SmolStr;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldNodeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ShapeNodeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SupportNodeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ShapeLeafId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SceneIdentitySourceId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DistanceSemantics {
    ExactSignedDistance,
    ConservativeLowerBound,
    UnknownOpaque,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SceneCaptureKind {
    Field,
    Shape,
    Region,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SceneTraceSafety {
    Exact,
    Conservative,
    Opaque,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SceneAnalysis {
    pub trace_safety: SceneTraceSafety,
    pub support_class: SupportClass,
    pub opaque_boundary: bool,
    pub can_coarse_support_pruning: bool,
    pub preserves_local_hit_context: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldNodeRecord {
    pub id: FieldNodeId,
    pub kind: FieldNodeKindSummary,
    pub target: Option<SmolStr>,
    pub children: Vec<FieldNodeId>,
    pub payload: Option<SceneOperatorPayload>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldNodeKindSummary {
    Use,
    Primitive(hir::FieldPrimitive),
    Union,
    Intersection,
    Subtract,
    Transform(TransformKind),
    Repeat(RepeatKind),
    Smooth(SmoothKind),
    Extrude,
    Revolve,
    Sweep,
    Loft,
    OpaqueLeaf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShapeNodeRecord {
    pub id: ShapeNodeId,
    pub kind: ShapeNodeKindSummary,
    pub target: Option<SmolStr>,
    pub children: Vec<ShapeNodeId>,
    pub leaf: Option<ShapeLeafId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShapeNodeKindSummary {
    Use,
    Union,
    Intersection,
    Subtract,
    Leaf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SupportNodeRecord {
    pub id: SupportNodeId,
    pub kind: SupportNodeKindSummary,
    pub target: Option<SmolStr>,
    pub children: Vec<SupportNodeId>,
    pub payload: Option<SupportPayload>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupportNodeKindSummary {
    Unknown,
    Unbounded,
    Use,
    Aabb,
    Sphere,
    Union,
    Intersection,
    Difference,
    Transform(TransformKind),
    Periodic(RepeatKind),
    Repeat(RepeatKind),
    OpaqueBoundary,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SceneOperatorPayload {
    Primitive {
        args: Option<Vec<SceneArgExpr>>,
    },
    Transform {
        param: Option<SceneValueExpr>,
    },
    Repeat {
        param: Option<SceneValueExpr>,
    },
    Smooth {
        smoothing: Option<SceneValueExpr>,
    },
    Extrude {
        height: Option<SceneValueExpr>,
        profile: Option<SceneProfileExpr>,
    },
    Revolve {
        profile: Option<SceneProfileExpr>,
    },
    Sweep {
        path: Option<SceneValueExpr>,
        profile: Option<SceneProfileExpr>,
    },
    Loft {
        height: Option<SceneValueExpr>,
        from: Option<SceneProfileExpr>,
        to: Option<SceneProfileExpr>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum SupportPayload {
    Aabb {
        min: SceneValueExpr,
        max: SceneValueExpr,
    },
    Sphere {
        center: SceneValueExpr,
        radius: SceneValueExpr,
    },
    Transform {
        param: Option<SceneValueExpr>,
    },
    Periodic {
        period: Option<SceneValueExpr>,
    },
    Repeat {
        param: Option<SceneValueExpr>,
    },
    OpaqueBoundary {
        bounds: Option<SceneValueExpr>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeLeafRef {
    pub scene: SmolStr,
    pub leaf: ShapeLeafId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SupportClass {
    Unknown,
    Bounded,
    Periodic,
    Unbounded,
}

impl From<hir::FieldSupport> for SupportClass {
    fn from(value: hir::FieldSupport) -> Self {
        match value {
            hir::FieldSupport::Unknown => SupportClass::Unknown,
            hir::FieldSupport::Bounded => SupportClass::Bounded,
            hir::FieldSupport::Periodic => SupportClass::Periodic,
            hir::FieldSupport::Unbounded => SupportClass::Unbounded,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TransformKind {
    Translate,
    Rotate,
    UniformScale,
    AffineTransform,
    Warp,
    Bend,
    Twist,
    Taper,
    Displace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RepeatKind {
    RepeatLinear,
    RepeatGrid,
    RadialRepeat,
    MirrorArray,
    InstanceArray,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SmoothKind {
    Union,
    Intersection,
    Subtract,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProfileOpKind {
    Extrude,
    Revolve,
    Sweep,
    Loft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ShapeMergeProvenancePolicy {
    Nearest,
    Ordered,
}

impl From<hir::ShapeMergeProvenancePolicy> for ShapeMergeProvenancePolicy {
    fn from(value: hir::ShapeMergeProvenancePolicy) -> Self {
        match value {
            hir::ShapeMergeProvenancePolicy::Nearest => ShapeMergeProvenancePolicy::Nearest,
            hir::ShapeMergeProvenancePolicy::Ordered => ShapeMergeProvenancePolicy::Ordered,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ShapeSubtractProvenancePolicy {
    Left,
    Right,
}

impl From<hir::ShapeSubtractProvenancePolicy> for ShapeSubtractProvenancePolicy {
    fn from(value: hir::ShapeSubtractProvenancePolicy) -> Self {
        match value {
            hir::ShapeSubtractProvenancePolicy::Left => ShapeSubtractProvenancePolicy::Left,
            hir::ShapeSubtractProvenancePolicy::Right => ShapeSubtractProvenancePolicy::Right,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ShapeNodeProvenancePolicy {
    Union(ShapeMergeProvenancePolicy),
    Intersection(ShapeMergeProvenancePolicy),
    Subtract(ShapeSubtractProvenancePolicy),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeNodeProvenanceRecord {
    pub node: ShapeNodeId,
    pub policy: ShapeNodeProvenancePolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SceneIdentitySourceKind {
    Repeat,
    Instance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneIdentitySourceRecord {
    pub id: SceneIdentitySourceId,
    pub node: FieldNodeId,
    pub kind: SceneIdentitySourceKind,
    pub repeat_kind: RepeatKind,
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

#[derive(Debug, Clone, PartialEq)]
pub enum SceneArgExpr {
    Positional(SceneValueExpr),
    Named {
        name: SmolStr,
        value: SceneValueExpr,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum SceneValueExpr {
    Literal(hir::Literal),
    List(Vec<SceneValueExpr>),
    Unary {
        op: hir::UnaryOp,
        expr: Box<SceneValueExpr>,
    },
    Binary {
        lhs: Box<SceneValueExpr>,
        op: hir::BinaryOp,
        rhs: Box<SceneValueExpr>,
    },
    Call {
        callee: SmolStr,
        args: Vec<SceneArgExpr>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum SceneProfileExpr {
    Primitive {
        primitive: hir::ProfilePrimitive,
        args: Vec<SceneArgExpr>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum SupportExpr {
    Unknown,
    Unbounded,
    Use {
        target: SmolStr,
    },
    Aabb {
        min: SceneValueExpr,
        max: SceneValueExpr,
    },
    Sphere {
        center: SceneValueExpr,
        radius: SceneValueExpr,
    },
    Union {
        items: Vec<SupportExpr>,
    },
    Intersection {
        items: Vec<SupportExpr>,
    },
    Difference {
        left: Box<SupportExpr>,
        right: Box<SupportExpr>,
    },
    Transform {
        kind: TransformKind,
        param: Option<SceneValueExpr>,
        inner: Box<SupportExpr>,
    },
    Periodic {
        kind: RepeatKind,
        period: Option<SceneValueExpr>,
        inner: Box<SupportExpr>,
    },
    Repeat {
        kind: RepeatKind,
        param: Option<SceneValueExpr>,
        inner: Box<SupportExpr>,
    },
    OpaqueBoundary {
        bounds: Option<SceneValueExpr>,
    },
}

impl SupportExpr {
    pub fn contains_opaque_leaf(&self) -> bool {
        match self {
            SupportExpr::OpaqueBoundary { .. } => true,
            SupportExpr::Union { items } | SupportExpr::Intersection { items } => {
                items.iter().any(SupportExpr::contains_opaque_leaf)
            }
            SupportExpr::Difference { left, right } => {
                left.contains_opaque_leaf() || right.contains_opaque_leaf()
            }
            SupportExpr::Transform { inner, .. }
            | SupportExpr::Periodic { inner, .. }
            | SupportExpr::Repeat { inner, .. } => inner.contains_opaque_leaf(),
            SupportExpr::Unknown
            | SupportExpr::Unbounded
            | SupportExpr::Use { .. }
            | SupportExpr::Aabb { .. }
            | SupportExpr::Sphere { .. } => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldNode {
    Use {
        target: SmolStr,
    },
    Primitive {
        primitive: hir::FieldPrimitive,
        args: Option<Vec<SceneArgExpr>>,
    },
    Union {
        items: Vec<FieldNode>,
    },
    Intersection {
        items: Vec<FieldNode>,
    },
    Subtract {
        left: Box<FieldNode>,
        right: Box<FieldNode>,
    },
    Transform {
        kind: TransformKind,
        param: Option<SceneValueExpr>,
        inner: Box<FieldNode>,
    },
    Repeat {
        kind: RepeatKind,
        param: Option<SceneValueExpr>,
        inner: Box<FieldNode>,
    },
    Smooth {
        kind: SmoothKind,
        smoothing: Option<SceneValueExpr>,
        items: Vec<FieldNode>,
    },
    Extrude {
        height: Option<SceneValueExpr>,
        profile: Option<SceneProfileExpr>,
    },
    Revolve {
        profile: Option<SceneProfileExpr>,
    },
    Sweep {
        path: Option<SceneValueExpr>,
        profile: Option<SceneProfileExpr>,
    },
    Loft {
        height: Option<SceneValueExpr>,
        from: Option<SceneProfileExpr>,
        to: Option<SceneProfileExpr>,
    },
    OpaqueLeaf,
}

impl FieldNode {
    pub fn contains_opaque_leaf(&self) -> bool {
        match self {
            FieldNode::OpaqueLeaf => true,
            FieldNode::Union { items }
            | FieldNode::Intersection { items }
            | FieldNode::Smooth { items, .. } => items.iter().any(FieldNode::contains_opaque_leaf),
            FieldNode::Subtract { left, right } => {
                left.contains_opaque_leaf() || right.contains_opaque_leaf()
            }
            FieldNode::Transform { inner, .. } | FieldNode::Repeat { inner, .. } => {
                inner.contains_opaque_leaf()
            }
            FieldNode::Use { .. }
            | FieldNode::Primitive { .. }
            | FieldNode::Extrude { .. }
            | FieldNode::Revolve { .. }
            | FieldNode::Sweep { .. }
            | FieldNode::Loft { .. } => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldScene {
    pub name: SmolStr,
    pub root: FieldNode,
    pub root_node_id: FieldNodeId,
    pub semantics: DistanceSemantics,
    pub support_class: SupportClass,
    pub support_expr: SupportExpr,
    pub root_support_id: SupportNodeId,
    pub authored_bounds: Option<SceneValueExpr>,
    pub opaque_boundary: bool,
    pub can_coarse_support_pruning: bool,
    pub analysis: SceneAnalysis,
    pub node_records: Vec<FieldNodeRecord>,
    pub support_records: Vec<SupportNodeRecord>,
    pub identity_sources: Vec<SceneIdentitySourceRecord>,
}

impl FieldScene {
    pub fn from_hir(
        name: SmolStr,
        graph: &hir::FieldGraph,
        body: Option<&hir::Body>,
        metadata: Option<&hir::FieldMetadata>,
    ) -> Self {
        let mut field_graphs = BTreeMap::new();
        field_graphs.insert(name.clone(), graph.clone());
        let mut field_bodies = BTreeMap::new();
        if let Some(body) = body {
            field_bodies.insert(name.clone(), body.clone());
        }
        let mut field_metadata = BTreeMap::new();
        if let Some(metadata) = metadata {
            field_metadata.insert(name.clone(), metadata.clone());
        }
        lower_field_scenes(&field_graphs, &field_bodies, &field_metadata)
            .remove(&name)
            .expect("field scene")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShapeLeafScene {
    pub id: ShapeLeafId,
    pub field: SmolStr,
    pub material: SmolStr,
    pub radiance: Option<SmolStr>,
    pub volume: Option<SmolStr>,
    pub payload: hir::Body,
    pub feature_id: u32,
    pub field_semantics: DistanceSemantics,
    pub opaque_boundary: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ShapeNode {
    Use {
        target: SmolStr,
    },
    Union {
        items: Vec<ShapeNode>,
    },
    Intersection {
        items: Vec<ShapeNode>,
    },
    Subtract {
        left: Box<ShapeNode>,
        right: Box<ShapeNode>,
    },
    Leaf(ShapeLeafScene),
}

impl ShapeNode {
    pub fn contains_opaque_leaf(&self) -> bool {
        match self {
            ShapeNode::Leaf(leaf) => {
                leaf.opaque_boundary
                    || matches!(leaf.field_semantics, DistanceSemantics::UnknownOpaque)
            }
            ShapeNode::Union { items } | ShapeNode::Intersection { items } => {
                items.iter().any(ShapeNode::contains_opaque_leaf)
            }
            ShapeNode::Subtract { left, right } => {
                left.contains_opaque_leaf() || right.contains_opaque_leaf()
            }
            ShapeNode::Use { .. } => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShapeScene {
    pub name: SmolStr,
    pub root: ShapeNode,
    pub root_node_id: ShapeNodeId,
    pub provenance: Option<ShapeProvenanceExpr>,
    pub provenance_records: Vec<ShapeNodeProvenanceRecord>,
    pub leaves: BTreeMap<ShapeLeafId, ShapeLeafScene>,
    pub feature_leaves: BTreeMap<u32, ShapeLeafRef>,
    pub support_expr: SupportExpr,
    pub root_support_id: SupportNodeId,
    pub semantics: DistanceSemantics,
    pub support_class: SupportClass,
    pub opaque_boundary: bool,
    pub can_coarse_support_pruning: bool,
    pub analysis: SceneAnalysis,
    pub node_records: Vec<ShapeNodeRecord>,
    pub support_records: Vec<SupportNodeRecord>,
}

impl ShapeScene {
    pub fn from_hir(
        name: SmolStr,
        graph: &hir::ShapeGraph,
        shape_graphs: &BTreeMap<SmolStr, hir::ShapeGraph>,
        fields: &BTreeMap<SmolStr, FieldScene>,
    ) -> Self {
        let mut all_shape_graphs = shape_graphs.clone();
        all_shape_graphs.insert(name.clone(), graph.clone());
        lower_shape_scenes(&all_shape_graphs, fields)
            .remove(&name)
            .expect("shape scene")
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SceneIrModule {
    pub fields: BTreeMap<SmolStr, FieldScene>,
    pub shapes: BTreeMap<SmolStr, ShapeScene>,
}

impl SceneIrModule {
    pub fn from_hir(module: &hir::Module) -> Self {
        let shape_graphs = module
            .shapes
            .iter()
            .filter_map(|(_, shape)| {
                shape
                    .graph
                    .as_ref()
                    .map(|graph| (shape.name.clone(), graph.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let field_graphs = module
            .functions
            .iter()
            .filter(|(_, func)| matches!(func.role, hir::FunctionRole::Field))
            .filter_map(|(_, func)| {
                func.field_graph
                    .as_ref()
                    .map(|graph| (func.name.clone(), graph.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let field_metadata = module
            .functions
            .iter()
            .filter(|(_, func)| matches!(func.role, hir::FunctionRole::Field))
            .filter_map(|(_, func)| {
                func.field
                    .clone()
                    .map(|metadata| (func.name.clone(), metadata))
            })
            .collect::<BTreeMap<_, _>>();
        let field_bodies = module
            .functions
            .iter()
            .filter(|(_, func)| matches!(func.role, hir::FunctionRole::Field))
            .filter_map(|(_, func)| func.body.clone().map(|body| (func.name.clone(), body)))
            .collect::<BTreeMap<_, _>>();
        let fields = lower_field_scenes(&field_graphs, &field_bodies, &field_metadata);
        let shapes = lower_shape_scenes(&shape_graphs, &fields);
        Self { fields, shapes }
    }

    pub fn has_opaque_leaves(&self) -> bool {
        self.fields.values().any(|field| field.opaque_boundary)
            || self.shapes.values().any(|shape| shape.opaque_boundary)
    }

    pub fn has_periodic_support(&self) -> bool {
        self.fields
            .values()
            .any(|field| matches!(field.support_class, SupportClass::Periodic))
            || self
                .shapes
                .values()
                .any(|shape| matches!(shape.support_class, SupportClass::Periodic))
    }

    pub fn has_bounded_support(&self) -> bool {
        self.fields
            .values()
            .any(|field| matches!(field.support_class, SupportClass::Bounded))
            || self
                .shapes
                .values()
                .any(|shape| matches!(shape.support_class, SupportClass::Bounded))
    }
}

impl FieldScene {
    pub fn field_node_record(&self, id: FieldNodeId) -> Option<&FieldNodeRecord> {
        self.node_records.iter().find(|record| record.id == id)
    }

    pub fn support_node_record(&self, id: SupportNodeId) -> Option<&SupportNodeRecord> {
        self.support_records.iter().find(|record| record.id == id)
    }
}

impl ShapeScene {
    pub fn shape_node_record(&self, id: ShapeNodeId) -> Option<&ShapeNodeRecord> {
        self.node_records.iter().find(|record| record.id == id)
    }

    pub fn support_node_record(&self, id: SupportNodeId) -> Option<&SupportNodeRecord> {
        self.support_records.iter().find(|record| record.id == id)
    }

    pub fn provenance_record(&self, id: ShapeNodeId) -> Option<&ShapeNodeProvenanceRecord> {
        self.provenance_records
            .iter()
            .find(|record| record.node == id)
    }
}

pub fn lower_module(module: &hir::Module) -> SceneIrModule {
    SceneIrModule::from_hir(module)
}

pub type SceneModule = SceneIrModule;

#[derive(Debug, Clone)]
struct FieldSceneDraft {
    root: FieldNode,
    support_expr: SupportExpr,
    authored_bounds: Option<SceneValueExpr>,
    declared_class: hir::FieldClass,
    authored_bounded: bool,
}

#[derive(Debug, Clone)]
struct ShapeSceneDraft {
    root: ShapeNode,
    provenance: Option<ShapeProvenanceExpr>,
    leaves: BTreeMap<ShapeLeafId, ShapeLeafScene>,
    support_expr: SupportExpr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CapabilitySummary {
    semantics: DistanceSemantics,
    support_class: SupportClass,
    opaque_boundary: bool,
    can_coarse_support_pruning: bool,
}

fn build_scene_analysis(
    summary: CapabilitySummary,
    preserves_local_hit_context: bool,
) -> SceneAnalysis {
    SceneAnalysis {
        trace_safety: match summary.semantics {
            DistanceSemantics::ExactSignedDistance => SceneTraceSafety::Exact,
            DistanceSemantics::ConservativeLowerBound => SceneTraceSafety::Conservative,
            DistanceSemantics::UnknownOpaque => SceneTraceSafety::Opaque,
        },
        support_class: summary.support_class,
        opaque_boundary: summary.opaque_boundary,
        can_coarse_support_pruning: summary.can_coarse_support_pruning,
        preserves_local_hit_context,
    }
}

fn build_field_node_records(
    root: &FieldNode,
) -> (
    FieldNodeId,
    Vec<FieldNodeRecord>,
    Vec<SceneIdentitySourceRecord>,
) {
    fn visit(
        node: &FieldNode,
        next_id: &mut u32,
        next_identity_id: &mut u32,
        out: &mut Vec<FieldNodeRecord>,
        identity_sources: &mut Vec<SceneIdentitySourceRecord>,
    ) -> FieldNodeId {
        let id = FieldNodeId(*next_id);
        *next_id += 1;
        let (kind, target, payload, children) = match node {
            FieldNode::Use { target } => (
                FieldNodeKindSummary::Use,
                Some(target.clone()),
                None,
                Vec::new(),
            ),
            FieldNode::Primitive { primitive, args } => (
                FieldNodeKindSummary::Primitive(*primitive),
                None,
                Some(SceneOperatorPayload::Primitive { args: args.clone() }),
                Vec::new(),
            ),
            FieldNode::Union { items } => (
                FieldNodeKindSummary::Union,
                None,
                None,
                items
                    .iter()
                    .map(|item| visit(item, next_id, next_identity_id, out, identity_sources))
                    .collect(),
            ),
            FieldNode::Intersection { items } => (
                FieldNodeKindSummary::Intersection,
                None,
                None,
                items
                    .iter()
                    .map(|item| visit(item, next_id, next_identity_id, out, identity_sources))
                    .collect(),
            ),
            FieldNode::Subtract { left, right } => (
                FieldNodeKindSummary::Subtract,
                None,
                None,
                vec![
                    visit(left, next_id, next_identity_id, out, identity_sources),
                    visit(right, next_id, next_identity_id, out, identity_sources),
                ],
            ),
            FieldNode::Transform { kind, param, inner } => (
                FieldNodeKindSummary::Transform(*kind),
                None,
                Some(SceneOperatorPayload::Transform {
                    param: param.clone(),
                }),
                vec![visit(
                    inner,
                    next_id,
                    next_identity_id,
                    out,
                    identity_sources,
                )],
            ),
            FieldNode::Repeat { kind, param, inner } => (
                FieldNodeKindSummary::Repeat(*kind),
                None,
                Some(SceneOperatorPayload::Repeat {
                    param: param.clone(),
                }),
                vec![visit(
                    inner,
                    next_id,
                    next_identity_id,
                    out,
                    identity_sources,
                )],
            ),
            FieldNode::Smooth {
                kind,
                smoothing,
                items,
            } => (
                FieldNodeKindSummary::Smooth(*kind),
                None,
                Some(SceneOperatorPayload::Smooth {
                    smoothing: smoothing.clone(),
                }),
                items
                    .iter()
                    .map(|item| visit(item, next_id, next_identity_id, out, identity_sources))
                    .collect(),
            ),
            FieldNode::Extrude { height, profile } => (
                FieldNodeKindSummary::Extrude,
                None,
                Some(SceneOperatorPayload::Extrude {
                    height: height.clone(),
                    profile: profile.clone(),
                }),
                Vec::new(),
            ),
            FieldNode::Revolve { profile } => (
                FieldNodeKindSummary::Revolve,
                None,
                Some(SceneOperatorPayload::Revolve {
                    profile: profile.clone(),
                }),
                Vec::new(),
            ),
            FieldNode::Sweep { path, profile } => (
                FieldNodeKindSummary::Sweep,
                None,
                Some(SceneOperatorPayload::Sweep {
                    path: path.clone(),
                    profile: profile.clone(),
                }),
                Vec::new(),
            ),
            FieldNode::Loft { height, from, to } => (
                FieldNodeKindSummary::Loft,
                None,
                Some(SceneOperatorPayload::Loft {
                    height: height.clone(),
                    from: from.clone(),
                    to: to.clone(),
                }),
                Vec::new(),
            ),
            FieldNode::OpaqueLeaf => (FieldNodeKindSummary::OpaqueLeaf, None, None, Vec::new()),
        };
        if let FieldNode::Repeat { kind, .. } = node {
            let identity_id = SceneIdentitySourceId(*next_identity_id);
            *next_identity_id += 1;
            identity_sources.push(SceneIdentitySourceRecord {
                id: identity_id,
                node: id,
                kind: match kind {
                    RepeatKind::InstanceArray => SceneIdentitySourceKind::Instance,
                    _ => SceneIdentitySourceKind::Repeat,
                },
                repeat_kind: *kind,
            });
        }
        out.push(FieldNodeRecord {
            id,
            kind,
            target,
            children,
            payload,
        });
        id
    }

    let mut out = Vec::new();
    let mut next_id = 0u32;
    let mut next_identity_id = 0u32;
    let mut identity_sources = Vec::new();
    let root_id = visit(
        root,
        &mut next_id,
        &mut next_identity_id,
        &mut out,
        &mut identity_sources,
    );
    (root_id, out, identity_sources)
}

fn build_shape_node_records(
    root: &ShapeNode,
    provenance: Option<&ShapeProvenanceExpr>,
) -> (
    ShapeNodeId,
    Vec<ShapeNodeRecord>,
    Vec<ShapeNodeProvenanceRecord>,
) {
    fn visit(
        node: &ShapeNode,
        provenance: Option<&ShapeProvenanceExpr>,
        next_id: &mut u32,
        out: &mut Vec<ShapeNodeRecord>,
        provenance_records: &mut Vec<ShapeNodeProvenanceRecord>,
    ) -> ShapeNodeId {
        let id = ShapeNodeId(*next_id);
        *next_id += 1;
        let (kind, target, children, leaf) = match node {
            ShapeNode::Use { target } => (
                ShapeNodeKindSummary::Use,
                Some(target.clone()),
                Vec::new(),
                None,
            ),
            ShapeNode::Union { items } => (
                ShapeNodeKindSummary::Union,
                None,
                items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| {
                        visit(
                            item,
                            provenance.and_then(|expr| match expr {
                                ShapeProvenanceExpr::Union { items, .. } => items.get(index),
                                _ => None,
                            }),
                            next_id,
                            out,
                            provenance_records,
                        )
                    })
                    .collect(),
                None,
            ),
            ShapeNode::Intersection { items } => (
                ShapeNodeKindSummary::Intersection,
                None,
                items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| {
                        visit(
                            item,
                            provenance.and_then(|expr| match expr {
                                ShapeProvenanceExpr::Intersection { items, .. } => items.get(index),
                                _ => None,
                            }),
                            next_id,
                            out,
                            provenance_records,
                        )
                    })
                    .collect(),
                None,
            ),
            ShapeNode::Subtract { left, right } => (
                ShapeNodeKindSummary::Subtract,
                None,
                vec![
                    visit(
                        left,
                        provenance.and_then(|expr| match expr {
                            ShapeProvenanceExpr::Subtract { left, .. } => Some(left.as_ref()),
                            _ => None,
                        }),
                        next_id,
                        out,
                        provenance_records,
                    ),
                    visit(
                        right,
                        provenance.and_then(|expr| match expr {
                            ShapeProvenanceExpr::Subtract { right, .. } => Some(right.as_ref()),
                            _ => None,
                        }),
                        next_id,
                        out,
                        provenance_records,
                    ),
                ],
                None,
            ),
            ShapeNode::Leaf(leaf) => (ShapeNodeKindSummary::Leaf, None, Vec::new(), Some(leaf.id)),
        };
        match provenance {
            Some(ShapeProvenanceExpr::Union { provenance, .. }) => {
                provenance_records.push(ShapeNodeProvenanceRecord {
                    node: id,
                    policy: ShapeNodeProvenancePolicy::Union(*provenance),
                });
            }
            Some(ShapeProvenanceExpr::Intersection { provenance, .. }) => {
                provenance_records.push(ShapeNodeProvenanceRecord {
                    node: id,
                    policy: ShapeNodeProvenancePolicy::Intersection(*provenance),
                });
            }
            Some(ShapeProvenanceExpr::Subtract { provenance, .. }) => {
                provenance_records.push(ShapeNodeProvenanceRecord {
                    node: id,
                    policy: ShapeNodeProvenancePolicy::Subtract(*provenance),
                });
            }
            _ => {}
        }
        out.push(ShapeNodeRecord {
            id,
            kind,
            target,
            children,
            leaf,
        });
        id
    }

    let mut out = Vec::new();
    let mut next_id = 0u32;
    let mut provenance_records = Vec::new();
    let root_id = visit(
        root,
        provenance,
        &mut next_id,
        &mut out,
        &mut provenance_records,
    );
    (root_id, out, provenance_records)
}

fn build_support_records(root: &SupportExpr) -> (SupportNodeId, Vec<SupportNodeRecord>) {
    fn visit(
        node: &SupportExpr,
        next_id: &mut u32,
        out: &mut Vec<SupportNodeRecord>,
    ) -> SupportNodeId {
        let id = SupportNodeId(*next_id);
        *next_id += 1;
        let (kind, target, payload, children) = match node {
            SupportExpr::Unknown => (SupportNodeKindSummary::Unknown, None, None, Vec::new()),
            SupportExpr::Unbounded => (SupportNodeKindSummary::Unbounded, None, None, Vec::new()),
            SupportExpr::Use { target } => (
                SupportNodeKindSummary::Use,
                Some(target.clone()),
                None,
                Vec::new(),
            ),
            SupportExpr::Aabb { min, max } => (
                SupportNodeKindSummary::Aabb,
                None,
                Some(SupportPayload::Aabb {
                    min: min.clone(),
                    max: max.clone(),
                }),
                Vec::new(),
            ),
            SupportExpr::Sphere { center, radius } => (
                SupportNodeKindSummary::Sphere,
                None,
                Some(SupportPayload::Sphere {
                    center: center.clone(),
                    radius: radius.clone(),
                }),
                Vec::new(),
            ),
            SupportExpr::Union { items } => (
                SupportNodeKindSummary::Union,
                None,
                None,
                items.iter().map(|item| visit(item, next_id, out)).collect(),
            ),
            SupportExpr::Intersection { items } => (
                SupportNodeKindSummary::Intersection,
                None,
                None,
                items.iter().map(|item| visit(item, next_id, out)).collect(),
            ),
            SupportExpr::Difference { left, right } => (
                SupportNodeKindSummary::Difference,
                None,
                None,
                vec![visit(left, next_id, out), visit(right, next_id, out)],
            ),
            SupportExpr::Transform { kind, param, inner } => (
                SupportNodeKindSummary::Transform(*kind),
                None,
                Some(SupportPayload::Transform {
                    param: param.clone(),
                }),
                vec![visit(inner, next_id, out)],
            ),
            SupportExpr::Periodic {
                kind,
                period,
                inner,
            } => (
                SupportNodeKindSummary::Periodic(*kind),
                None,
                Some(SupportPayload::Periodic {
                    period: period.clone(),
                }),
                vec![visit(inner, next_id, out)],
            ),
            SupportExpr::Repeat { kind, param, inner } => (
                SupportNodeKindSummary::Repeat(*kind),
                None,
                Some(SupportPayload::Repeat {
                    param: param.clone(),
                }),
                vec![visit(inner, next_id, out)],
            ),
            SupportExpr::OpaqueBoundary { bounds } => (
                SupportNodeKindSummary::OpaqueBoundary,
                None,
                Some(SupportPayload::OpaqueBoundary {
                    bounds: bounds.clone(),
                }),
                Vec::new(),
            ),
        };
        out.push(SupportNodeRecord {
            id,
            kind,
            target,
            children,
            payload,
        });
        id
    }

    let mut out = Vec::new();
    let mut next_id = 0u32;
    let root_id = visit(root, &mut next_id, &mut out);
    (root_id, out)
}

fn build_shape_feature_leaf_index(
    shape: &SmolStr,
    drafts: &BTreeMap<SmolStr, ShapeSceneDraft>,
) -> BTreeMap<u32, ShapeLeafRef> {
    fn visit(
        scene_name: &SmolStr,
        node: &ShapeNode,
        drafts: &BTreeMap<SmolStr, ShapeSceneDraft>,
        visiting: &mut BTreeSet<SmolStr>,
        out: &mut BTreeMap<u32, ShapeLeafRef>,
    ) {
        match node {
            ShapeNode::Use { target } => {
                if !visiting.insert(target.clone()) {
                    return;
                }
                if let Some(draft) = drafts.get(target) {
                    visit(target, &draft.root, drafts, visiting, out);
                }
                visiting.remove(target);
            }
            ShapeNode::Leaf(leaf) => {
                out.insert(
                    leaf.feature_id,
                    ShapeLeafRef {
                        scene: scene_name.clone(),
                        leaf: leaf.id,
                    },
                );
            }
            ShapeNode::Union { items } | ShapeNode::Intersection { items } => {
                for item in items {
                    visit(scene_name, item, drafts, visiting, out);
                }
            }
            ShapeNode::Subtract { left, right } => {
                visit(scene_name, left, drafts, visiting, out);
                visit(scene_name, right, drafts, visiting, out);
            }
        }
    }

    let mut out = BTreeMap::new();
    let mut visiting = BTreeSet::from([shape.clone()]);
    if let Some(draft) = drafts.get(shape) {
        visit(shape, &draft.root, drafts, &mut visiting, &mut out);
    }
    out
}

pub fn lower_field_scenes(
    field_graphs: &BTreeMap<SmolStr, hir::FieldGraph>,
    field_bodies: &BTreeMap<SmolStr, hir::Body>,
    field_metadata: &BTreeMap<SmolStr, hir::FieldMetadata>,
) -> BTreeMap<SmolStr, FieldScene> {
    let drafts = field_graphs
        .iter()
        .map(|(name, graph)| {
            let metadata = field_metadata.get(name);
            let field_body = field_bodies.get(name);
            let root = lower_field_node(&graph.root, field_body);
            let support_expr = lower_support_expr(&graph.root, field_body, metadata);
            (
                name.clone(),
                FieldSceneDraft {
                    root,
                    support_expr,
                    authored_bounds: metadata.and_then(lower_authored_bounds_expr),
                    declared_class: metadata
                        .map(|field| field.class)
                        .unwrap_or(hir::FieldClass::Conservative),
                    authored_bounded: metadata.is_some_and(|field| {
                        field.authored_support.is_some() || field.authored_bounds.is_some()
                    }),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut cache = BTreeMap::new();
    let mut fields = BTreeMap::new();
    for (name, _graph) in field_graphs {
        let summary = analyze_field_scene(name, &drafts, &mut cache, &mut BTreeSet::new());
        let draft = drafts.get(name).expect("field draft");
        let analysis = build_scene_analysis(summary, true);
        let (root_node_id, node_records, identity_sources) = build_field_node_records(&draft.root);
        let (root_support_id, support_records) = build_support_records(&draft.support_expr);
        fields.insert(
            name.clone(),
            FieldScene {
                name: name.clone(),
                root: draft.root.clone(),
                root_node_id,
                semantics: summary.semantics,
                support_class: summary.support_class,
                support_expr: draft.support_expr.clone(),
                root_support_id,
                authored_bounds: draft.authored_bounds.clone(),
                opaque_boundary: summary.opaque_boundary,
                can_coarse_support_pruning: summary.can_coarse_support_pruning,
                analysis,
                node_records,
                support_records,
                identity_sources,
            },
        );
    }
    fields
}

pub fn lower_shape_scenes(
    shape_graphs: &BTreeMap<SmolStr, hir::ShapeGraph>,
    fields: &BTreeMap<SmolStr, FieldScene>,
) -> BTreeMap<SmolStr, ShapeScene> {
    let mut support_cache = BTreeMap::new();
    let mut support_visiting = BTreeSet::new();
    let drafts = shape_graphs
        .iter()
        .map(|(name, graph)| {
            let mut next_leaf_id = 0u32;
            let mut leaves = BTreeMap::new();
            (
                name.clone(),
                ShapeSceneDraft {
                    root: lower_shape_node(&graph.root, fields, &mut next_leaf_id, &mut leaves),
                    provenance: graph.provenance.as_ref().map(lower_shape_provenance_expr),
                    leaves,
                    support_expr: lower_shape_support_expr(
                        &graph.root,
                        shape_graphs,
                        fields,
                        &mut support_cache,
                        &mut support_visiting,
                    ),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut cache = BTreeMap::new();
    let mut shapes = BTreeMap::new();
    for (name, _graph) in shape_graphs {
        let summary = analyze_shape_scene(name, &drafts, fields, &mut cache, &mut BTreeSet::new());
        let draft = drafts.get(name).expect("shape draft");
        let analysis = build_scene_analysis(summary, true);
        let (root_node_id, node_records, provenance_records) =
            build_shape_node_records(&draft.root, draft.provenance.as_ref());
        let (root_support_id, support_records) = build_support_records(&draft.support_expr);
        shapes.insert(
            name.clone(),
            ShapeScene {
                name: name.clone(),
                root: draft.root.clone(),
                root_node_id,
                provenance: draft.provenance.clone(),
                provenance_records,
                leaves: draft.leaves.clone(),
                feature_leaves: build_shape_feature_leaf_index(name, &drafts),
                support_expr: draft.support_expr.clone(),
                root_support_id,
                semantics: summary.semantics,
                support_class: summary.support_class,
                opaque_boundary: summary.opaque_boundary,
                can_coarse_support_pruning: summary.can_coarse_support_pruning,
                analysis,
                node_records,
                support_records,
            },
        );
    }
    shapes
}

fn conservative_unknown_support() -> CapabilitySummary {
    CapabilitySummary {
        semantics: DistanceSemantics::ConservativeLowerBound,
        support_class: SupportClass::Unknown,
        opaque_boundary: false,
        can_coarse_support_pruning: false,
    }
}

fn opaque_capabilities(support_class: SupportClass) -> CapabilitySummary {
    CapabilitySummary {
        semantics: DistanceSemantics::UnknownOpaque,
        support_class,
        opaque_boundary: true,
        can_coarse_support_pruning: false,
    }
}

fn primitive_capabilities(primitive: hir::FieldPrimitive) -> CapabilitySummary {
    use hir::FieldPrimitive::{
        Box, BoxFrame, CappedCone, Capsule, Cone, Cylinder, Ellipsoid, HexPrism, Plane, RoundedBox,
        Slab, Sphere, Torus, TrianglePrism,
    };

    match primitive {
        Sphere | Box | Capsule | Cylinder | Torus | RoundedBox | CappedCone | BoxFrame
        | TrianglePrism | HexPrism => CapabilitySummary {
            semantics: DistanceSemantics::ExactSignedDistance,
            support_class: SupportClass::Bounded,
            opaque_boundary: false,
            can_coarse_support_pruning: true,
        },
        Ellipsoid => CapabilitySummary {
            semantics: DistanceSemantics::ConservativeLowerBound,
            support_class: SupportClass::Bounded,
            opaque_boundary: false,
            can_coarse_support_pruning: true,
        },
        Plane | Cone | Slab => CapabilitySummary {
            semantics: DistanceSemantics::ExactSignedDistance,
            support_class: SupportClass::Unbounded,
            opaque_boundary: false,
            can_coarse_support_pruning: false,
        },
    }
}

fn profile_op_capabilities() -> CapabilitySummary {
    CapabilitySummary {
        semantics: DistanceSemantics::ConservativeLowerBound,
        support_class: SupportClass::Bounded,
        opaque_boundary: false,
        can_coarse_support_pruning: true,
    }
}

fn apply_declared_class(
    semantics: DistanceSemantics,
    declared_class: hir::FieldClass,
) -> DistanceSemantics {
    match (semantics, declared_class) {
        (DistanceSemantics::UnknownOpaque, _) => DistanceSemantics::UnknownOpaque,
        (_, hir::FieldClass::Conservative) => DistanceSemantics::ConservativeLowerBound,
        (other, hir::FieldClass::Exact) => other,
    }
}

fn apply_authored_support(summary: CapabilitySummary, authored_bounded: bool) -> CapabilitySummary {
    if !authored_bounded || summary.opaque_boundary {
        return summary;
    }
    match summary.support_class {
        SupportClass::Unknown | SupportClass::Bounded => CapabilitySummary {
            support_class: SupportClass::Bounded,
            can_coarse_support_pruning: true,
            ..summary
        },
        SupportClass::Periodic | SupportClass::Unbounded => summary,
    }
}

fn merge_union_support(items: &[CapabilitySummary]) -> SupportClass {
    if items
        .iter()
        .any(|item| matches!(item.support_class, SupportClass::Unbounded))
    {
        SupportClass::Unbounded
    } else if items
        .iter()
        .any(|item| matches!(item.support_class, SupportClass::Periodic))
    {
        SupportClass::Periodic
    } else if items
        .iter()
        .any(|item| matches!(item.support_class, SupportClass::Unknown))
    {
        SupportClass::Unknown
    } else if items.is_empty() {
        SupportClass::Unknown
    } else {
        SupportClass::Bounded
    }
}

fn merge_intersection_support(items: &[CapabilitySummary]) -> SupportClass {
    if items
        .iter()
        .any(|item| matches!(item.support_class, SupportClass::Bounded))
    {
        SupportClass::Bounded
    } else if items
        .iter()
        .any(|item| matches!(item.support_class, SupportClass::Unknown))
    {
        SupportClass::Unknown
    } else if items
        .iter()
        .any(|item| matches!(item.support_class, SupportClass::Periodic))
    {
        SupportClass::Periodic
    } else if items
        .iter()
        .any(|item| matches!(item.support_class, SupportClass::Unbounded))
    {
        SupportClass::Unbounded
    } else {
        SupportClass::Unknown
    }
}

fn merge_boolean_capabilities(
    items: &[CapabilitySummary],
    support_class: SupportClass,
) -> CapabilitySummary {
    let opaque_boundary = items.iter().any(|item| item.opaque_boundary);
    if opaque_boundary {
        return opaque_capabilities(support_class);
    }
    CapabilitySummary {
        semantics: DistanceSemantics::ConservativeLowerBound,
        support_class,
        opaque_boundary: false,
        can_coarse_support_pruning: matches!(support_class, SupportClass::Bounded)
            && items.iter().all(|item| item.can_coarse_support_pruning),
    }
}

fn transform_capabilities(kind: TransformKind, inner: CapabilitySummary) -> CapabilitySummary {
    if inner.opaque_boundary {
        return opaque_capabilities(inner.support_class);
    }
    let semantics = match kind {
        TransformKind::Translate | TransformKind::Rotate | TransformKind::UniformScale => {
            inner.semantics
        }
        TransformKind::AffineTransform
        | TransformKind::Warp
        | TransformKind::Bend
        | TransformKind::Twist
        | TransformKind::Taper
        | TransformKind::Displace => DistanceSemantics::ConservativeLowerBound,
    };
    let can_coarse_support_pruning = matches!(
        kind,
        TransformKind::Translate | TransformKind::Rotate | TransformKind::UniformScale
    ) && inner.can_coarse_support_pruning
        && matches!(inner.support_class, SupportClass::Bounded);
    CapabilitySummary {
        semantics,
        support_class: inner.support_class,
        opaque_boundary: false,
        can_coarse_support_pruning,
    }
}

fn repeat_capabilities(kind: RepeatKind, inner: CapabilitySummary) -> CapabilitySummary {
    if inner.opaque_boundary {
        return opaque_capabilities(match kind {
            RepeatKind::RepeatLinear | RepeatKind::RepeatGrid | RepeatKind::RadialRepeat => {
                SupportClass::Periodic
            }
            RepeatKind::MirrorArray | RepeatKind::InstanceArray => inner.support_class,
        });
    }
    let support_class = match kind {
        RepeatKind::RepeatLinear | RepeatKind::RepeatGrid | RepeatKind::RadialRepeat => {
            SupportClass::Periodic
        }
        RepeatKind::MirrorArray | RepeatKind::InstanceArray => inner.support_class,
    };
    let can_coarse_support_pruning = match kind {
        RepeatKind::MirrorArray | RepeatKind::InstanceArray => {
            inner.can_coarse_support_pruning && matches!(support_class, SupportClass::Bounded)
        }
        RepeatKind::RepeatLinear | RepeatKind::RepeatGrid | RepeatKind::RadialRepeat => false,
    };
    CapabilitySummary {
        semantics: DistanceSemantics::ConservativeLowerBound,
        support_class,
        opaque_boundary: false,
        can_coarse_support_pruning,
    }
}

fn analyze_field_scene(
    name: &SmolStr,
    drafts: &BTreeMap<SmolStr, FieldSceneDraft>,
    cache: &mut BTreeMap<SmolStr, CapabilitySummary>,
    visiting: &mut BTreeSet<SmolStr>,
) -> CapabilitySummary {
    if let Some(summary) = cache.get(name).copied() {
        return summary;
    }
    if !visiting.insert(name.clone()) {
        return conservative_unknown_support();
    }
    let summary = drafts
        .get(name)
        .map_or_else(conservative_unknown_support, |draft| {
            let base = analyze_field_node(&draft.root, drafts, cache, visiting);
            apply_authored_support(
                CapabilitySummary {
                    semantics: apply_declared_class(base.semantics, draft.declared_class),
                    ..base
                },
                draft.authored_bounded,
            )
        });
    visiting.remove(name);
    cache.insert(name.clone(), summary);
    summary
}

fn analyze_field_node(
    node: &FieldNode,
    drafts: &BTreeMap<SmolStr, FieldSceneDraft>,
    cache: &mut BTreeMap<SmolStr, CapabilitySummary>,
    visiting: &mut BTreeSet<SmolStr>,
) -> CapabilitySummary {
    match node {
        FieldNode::Use { target } => analyze_field_scene(target, drafts, cache, visiting),
        FieldNode::Primitive { primitive, .. } => primitive_capabilities(*primitive),
        FieldNode::Union { items } => {
            let items = items
                .iter()
                .map(|item| analyze_field_node(item, drafts, cache, visiting))
                .collect::<Vec<_>>();
            merge_boolean_capabilities(&items, merge_union_support(&items))
        }
        FieldNode::Intersection { items } => {
            let items = items
                .iter()
                .map(|item| analyze_field_node(item, drafts, cache, visiting))
                .collect::<Vec<_>>();
            merge_boolean_capabilities(&items, merge_intersection_support(&items))
        }
        FieldNode::Subtract { left, right } => {
            let left = analyze_field_node(left, drafts, cache, visiting);
            let right = analyze_field_node(right, drafts, cache, visiting);
            if left.opaque_boundary || right.opaque_boundary {
                opaque_capabilities(left.support_class)
            } else {
                CapabilitySummary {
                    semantics: DistanceSemantics::ConservativeLowerBound,
                    support_class: left.support_class,
                    opaque_boundary: false,
                    can_coarse_support_pruning: matches!(left.support_class, SupportClass::Bounded)
                        && left.can_coarse_support_pruning,
                }
            }
        }
        FieldNode::Transform { kind, inner, .. } => {
            transform_capabilities(*kind, analyze_field_node(inner, drafts, cache, visiting))
        }
        FieldNode::Repeat { kind, inner, .. } => {
            repeat_capabilities(*kind, analyze_field_node(inner, drafts, cache, visiting))
        }
        FieldNode::Smooth { kind, items, .. } => {
            let items = items
                .iter()
                .map(|item| analyze_field_node(item, drafts, cache, visiting))
                .collect::<Vec<_>>();
            let support_class = match kind {
                SmoothKind::Union => merge_union_support(&items),
                SmoothKind::Intersection => merge_intersection_support(&items),
                SmoothKind::Subtract => items
                    .first()
                    .map(|item| item.support_class)
                    .unwrap_or(SupportClass::Unknown),
            };
            merge_boolean_capabilities(&items, support_class)
        }
        FieldNode::Extrude { .. }
        | FieldNode::Revolve { .. }
        | FieldNode::Sweep { .. }
        | FieldNode::Loft { .. } => profile_op_capabilities(),
        FieldNode::OpaqueLeaf => opaque_capabilities(SupportClass::Unknown),
    }
}

fn analyze_shape_scene(
    name: &SmolStr,
    drafts: &BTreeMap<SmolStr, ShapeSceneDraft>,
    fields: &BTreeMap<SmolStr, FieldScene>,
    cache: &mut BTreeMap<SmolStr, CapabilitySummary>,
    visiting: &mut BTreeSet<SmolStr>,
) -> CapabilitySummary {
    if let Some(summary) = cache.get(name).copied() {
        return summary;
    }
    if !visiting.insert(name.clone()) {
        return conservative_unknown_support();
    }
    let summary = drafts
        .get(name)
        .map_or_else(conservative_unknown_support, |draft| {
            analyze_shape_node(&draft.root, drafts, fields, cache, visiting)
        });
    visiting.remove(name);
    cache.insert(name.clone(), summary);
    summary
}

fn analyze_shape_node(
    node: &ShapeNode,
    drafts: &BTreeMap<SmolStr, ShapeSceneDraft>,
    fields: &BTreeMap<SmolStr, FieldScene>,
    cache: &mut BTreeMap<SmolStr, CapabilitySummary>,
    visiting: &mut BTreeSet<SmolStr>,
) -> CapabilitySummary {
    match node {
        ShapeNode::Use { target } => analyze_shape_scene(target, drafts, fields, cache, visiting),
        ShapeNode::Leaf(leaf) => {
            fields
                .get(&leaf.field)
                .map_or_else(conservative_unknown_support, |field| CapabilitySummary {
                    semantics: field.semantics,
                    support_class: field.support_class,
                    opaque_boundary: field.opaque_boundary,
                    can_coarse_support_pruning: field.can_coarse_support_pruning,
                })
        }
        ShapeNode::Union { items } => {
            let items = items
                .iter()
                .map(|item| analyze_shape_node(item, drafts, fields, cache, visiting))
                .collect::<Vec<_>>();
            merge_boolean_capabilities(&items, merge_union_support(&items))
        }
        ShapeNode::Intersection { items } => {
            let items = items
                .iter()
                .map(|item| analyze_shape_node(item, drafts, fields, cache, visiting))
                .collect::<Vec<_>>();
            merge_boolean_capabilities(&items, merge_intersection_support(&items))
        }
        ShapeNode::Subtract { left, right } => {
            let left = analyze_shape_node(left, drafts, fields, cache, visiting);
            let right = analyze_shape_node(right, drafts, fields, cache, visiting);
            if left.opaque_boundary || right.opaque_boundary {
                opaque_capabilities(left.support_class)
            } else {
                CapabilitySummary {
                    semantics: DistanceSemantics::ConservativeLowerBound,
                    support_class: left.support_class,
                    opaque_boundary: false,
                    can_coarse_support_pruning: matches!(left.support_class, SupportClass::Bounded)
                        && left.can_coarse_support_pruning,
                }
            }
        }
    }
}

fn lower_field_node(expr: &hir::FieldExpr, field_body: Option<&hir::Body>) -> FieldNode {
    match expr {
        hir::FieldExpr::Use { target } => FieldNode::Use {
            target: target.clone(),
        },
        hir::FieldExpr::Primitive { primitive, args } => FieldNode::Primitive {
            primitive: *primitive,
            args: lower_scene_args(args, field_body),
        },
        hir::FieldExpr::Union { items } => FieldNode::Union {
            items: items
                .iter()
                .map(|item| lower_field_node(item, field_body))
                .collect(),
        },
        hir::FieldExpr::Intersection { items } => FieldNode::Intersection {
            items: items
                .iter()
                .map(|item| lower_field_node(item, field_body))
                .collect(),
        },
        hir::FieldExpr::Subtract { left, right } => FieldNode::Subtract {
            left: Box::new(lower_field_node(left, field_body)),
            right: Box::new(lower_field_node(right, field_body)),
        },
        hir::FieldExpr::Translate { translate, body } => FieldNode::Transform {
            kind: TransformKind::Translate,
            param: lower_scene_body_expr(translate),
            inner: Box::new(lower_field_node(body, field_body)),
        },
        hir::FieldExpr::Rotate { rotate, body } => FieldNode::Transform {
            kind: TransformKind::Rotate,
            param: lower_scene_body_expr(rotate),
            inner: Box::new(lower_field_node(body, field_body)),
        },
        hir::FieldExpr::UniformScale { scale, body } => FieldNode::Transform {
            kind: TransformKind::UniformScale,
            param: lower_scene_body_expr(scale),
            inner: Box::new(lower_field_node(body, field_body)),
        },
        hir::FieldExpr::AffineTransform { transform, body } => FieldNode::Transform {
            kind: TransformKind::AffineTransform,
            param: lower_scene_body_expr(transform),
            inner: Box::new(lower_field_node(body, field_body)),
        },
        hir::FieldExpr::Warp { warp, body } => FieldNode::Transform {
            kind: TransformKind::Warp,
            param: lower_scene_body_expr(warp),
            inner: Box::new(lower_field_node(body, field_body)),
        },
        hir::FieldExpr::RepeatLinear { repeat, body } => FieldNode::Repeat {
            kind: RepeatKind::RepeatLinear,
            param: lower_scene_body_expr(repeat),
            inner: Box::new(lower_field_node(body, field_body)),
        },
        hir::FieldExpr::RepeatGrid { repeat, body } => FieldNode::Repeat {
            kind: RepeatKind::RepeatGrid,
            param: lower_scene_body_expr(repeat),
            inner: Box::new(lower_field_node(body, field_body)),
        },
        hir::FieldExpr::RadialRepeat { radial, body } => FieldNode::Repeat {
            kind: RepeatKind::RadialRepeat,
            param: lower_scene_body_expr(radial),
            inner: Box::new(lower_field_node(body, field_body)),
        },
        hir::FieldExpr::MirrorArray { mirror, body } => FieldNode::Repeat {
            kind: RepeatKind::MirrorArray,
            param: lower_scene_body_expr(mirror),
            inner: Box::new(lower_field_node(body, field_body)),
        },
        hir::FieldExpr::InstanceArray { instance, body } => FieldNode::Repeat {
            kind: RepeatKind::InstanceArray,
            param: lower_scene_body_expr(instance),
            inner: Box::new(lower_field_node(body, field_body)),
        },
        hir::FieldExpr::SmoothUnion { smoothing, items } => FieldNode::Smooth {
            kind: SmoothKind::Union,
            smoothing: lower_scene_body_expr(smoothing),
            items: items
                .iter()
                .map(|item| lower_field_node(item, field_body))
                .collect(),
        },
        hir::FieldExpr::SmoothIntersection { smoothing, items } => FieldNode::Smooth {
            kind: SmoothKind::Intersection,
            smoothing: lower_scene_body_expr(smoothing),
            items: items
                .iter()
                .map(|item| lower_field_node(item, field_body))
                .collect(),
        },
        hir::FieldExpr::SmoothSubtract {
            smoothing,
            left,
            right,
        } => FieldNode::Smooth {
            kind: SmoothKind::Subtract,
            smoothing: lower_scene_body_expr(smoothing),
            items: vec![
                lower_field_node(left, field_body),
                lower_field_node(right, field_body),
            ],
        },
        hir::FieldExpr::Bend { bend, body } => FieldNode::Transform {
            kind: TransformKind::Bend,
            param: lower_scene_body_expr(bend),
            inner: Box::new(lower_field_node(body, field_body)),
        },
        hir::FieldExpr::Twist { twist, body } => FieldNode::Transform {
            kind: TransformKind::Twist,
            param: lower_scene_body_expr(twist),
            inner: Box::new(lower_field_node(body, field_body)),
        },
        hir::FieldExpr::Taper { taper, body } => FieldNode::Transform {
            kind: TransformKind::Taper,
            param: lower_scene_body_expr(taper),
            inner: Box::new(lower_field_node(body, field_body)),
        },
        hir::FieldExpr::Displace { displace, body } => FieldNode::Transform {
            kind: TransformKind::Displace,
            param: lower_scene_body_expr(displace),
            inner: Box::new(lower_field_node(body, field_body)),
        },
        hir::FieldExpr::Extrude { height, profile } => FieldNode::Extrude {
            height: lower_scene_body_expr(height),
            profile: lower_scene_profile_expr(profile, field_body),
        },
        hir::FieldExpr::Revolve { profile } => FieldNode::Revolve {
            profile: lower_scene_profile_expr(profile, field_body),
        },
        hir::FieldExpr::Sweep { path, profile } => FieldNode::Sweep {
            path: lower_scene_body_expr(path),
            profile: lower_scene_profile_expr(profile, field_body),
        },
        hir::FieldExpr::Loft { height, from, to } => FieldNode::Loft {
            height: lower_scene_body_expr(height),
            from: lower_scene_profile_expr(from, field_body),
            to: lower_scene_profile_expr(to, field_body),
        },
        hir::FieldExpr::Custom { .. } => FieldNode::OpaqueLeaf,
    }
}

fn lower_scene_profile_expr(
    profile: &hir::ProfileExpr,
    body: Option<&hir::Body>,
) -> Option<SceneProfileExpr> {
    match profile {
        hir::ProfileExpr::Primitive { primitive, args } => Some(SceneProfileExpr::Primitive {
            primitive: *primitive,
            args: lower_scene_args(args, body)?,
        }),
    }
}

fn lower_scene_args(args: &[hir::Arg], body: Option<&hir::Body>) -> Option<Vec<SceneArgExpr>> {
    args.iter()
        .map(|arg| match arg {
            hir::Arg::Positional { value, .. } => {
                let body = body?;
                Some(SceneArgExpr::Positional(lower_scene_expr(body, *value)?))
            }
            hir::Arg::Named { name, value, .. } => {
                let body = body?;
                Some(SceneArgExpr::Named {
                    name: name.clone(),
                    value: lower_scene_expr(body, *value)?,
                })
            }
        })
        .collect()
}

fn lower_scene_body_expr(body: &hir::Body) -> Option<SceneValueExpr> {
    if body.root_stmts.len() != 1 {
        return None;
    }
    let stmt = *body.root_stmts.first()?;
    match &body.stmts[stmt] {
        hir::Stmt::Expr(expr) | hir::Stmt::Return(Some(expr)) => lower_scene_expr(body, *expr),
        _ => None,
    }
}

fn lower_scene_expr(body: &hir::Body, expr: hir::Idx<hir::Expr>) -> Option<SceneValueExpr> {
    match &body.exprs[expr] {
        hir::Expr::Literal(literal) => Some(SceneValueExpr::Literal(literal.clone())),
        hir::Expr::List(items) => Some(SceneValueExpr::List(
            items
                .iter()
                .map(|item| lower_scene_expr(body, *item))
                .collect::<Option<Vec<_>>>()?,
        )),
        hir::Expr::Unary { op, expr, .. } => Some(SceneValueExpr::Unary {
            op: *op,
            expr: Box::new(lower_scene_expr(body, *expr)?),
        }),
        hir::Expr::Binary { lhs, op, rhs, .. } => Some(SceneValueExpr::Binary {
            lhs: Box::new(lower_scene_expr(body, *lhs)?),
            op: *op,
            rhs: Box::new(lower_scene_expr(body, *rhs)?),
        }),
        hir::Expr::Call {
            callee,
            args,
            type_args,
        } if type_args.is_empty() => {
            let hir::Expr::Variable(callee) = &body.exprs[*callee] else {
                return None;
            };
            Some(SceneValueExpr::Call {
                callee: callee.clone(),
                args: lower_scene_args(args, Some(body))?,
            })
        }
        _ => None,
    }
}

fn lower_authored_bounds_expr(metadata: &hir::FieldMetadata) -> Option<SceneValueExpr> {
    if let Some(bounds) = metadata.authored_bounds.as_ref() {
        return lower_scene_body_expr(bounds);
    }
    let support = metadata.authored_support.as_ref()?;
    extract_support_bounds_expr(&lower_scene_body_expr(support)?)
}

fn extract_support_bounds_expr(value: &SceneValueExpr) -> Option<SceneValueExpr> {
    let SceneValueExpr::Call { callee, args } = value else {
        return None;
    };
    if callee.as_str() != "Support3" {
        return None;
    }
    args.iter().find_map(|arg| match arg {
        SceneArgExpr::Named { name, value } if name.as_str() == "bounds" => Some(value.clone()),
        _ => None,
    })
}

fn literal_f32(value: f32) -> SceneValueExpr {
    SceneValueExpr::Literal(hir::Literal::Float(value.into()))
}

fn literal_vec3(x: f32, y: f32, z: f32) -> SceneValueExpr {
    SceneValueExpr::Call {
        callee: SmolStr::new("vec3"),
        args: vec![
            SceneArgExpr::Positional(literal_f32(x)),
            SceneArgExpr::Positional(literal_f32(y)),
            SceneArgExpr::Positional(literal_f32(z)),
        ],
    }
}

fn negate_expr(value: SceneValueExpr) -> SceneValueExpr {
    SceneValueExpr::Unary {
        op: hir::UnaryOp::Neg,
        expr: Box::new(value),
    }
}

fn get_named_scene_arg(
    args: &[hir::Arg],
    body: Option<&hir::Body>,
    name: &str,
) -> Option<SceneValueExpr> {
    args.iter().find_map(|arg| match arg {
        hir::Arg::Named {
            name: arg_name,
            value,
            ..
        } if arg_name.as_str() == name => {
            let body = body?;
            lower_scene_expr(body, *value)
        }
        _ => None,
    })
}

fn lower_primitive_support_expr(
    primitive: hir::FieldPrimitive,
    args: &[hir::Arg],
    body: Option<&hir::Body>,
) -> SupportExpr {
    match primitive {
        hir::FieldPrimitive::Sphere => SupportExpr::Sphere {
            center: literal_vec3(0.0, 0.0, 0.0),
            radius: get_named_scene_arg(args, body, "radius").unwrap_or_else(|| literal_f32(1.0)),
        },
        hir::FieldPrimitive::Box
        | hir::FieldPrimitive::RoundedBox
        | hir::FieldPrimitive::BoxFrame => {
            let half = get_named_scene_arg(args, body, "half_size")
                .unwrap_or_else(|| literal_vec3(1.0, 1.0, 1.0));
            SupportExpr::Aabb {
                min: negate_expr(half.clone()),
                max: half,
            }
        }
        hir::FieldPrimitive::Capsule
        | hir::FieldPrimitive::Cylinder
        | hir::FieldPrimitive::CappedCone
        | hir::FieldPrimitive::Ellipsoid
        | hir::FieldPrimitive::Torus
        | hir::FieldPrimitive::TrianglePrism
        | hir::FieldPrimitive::HexPrism => {
            let radius = get_named_scene_arg(args, body, "radius")
                .or_else(|| get_named_scene_arg(args, body, "major_radius"))
                .unwrap_or_else(|| literal_f32(1.0));
            SupportExpr::Sphere {
                center: literal_vec3(0.0, 0.0, 0.0),
                radius,
            }
        }
        hir::FieldPrimitive::Plane | hir::FieldPrimitive::Cone | hir::FieldPrimitive::Slab => {
            SupportExpr::Unbounded
        }
    }
}

fn lower_profile_support_expr(metadata: Option<&hir::FieldMetadata>) -> SupportExpr {
    metadata
        .and_then(lower_authored_bounds_expr)
        .and_then(|bounds| match bounds {
            SceneValueExpr::Call { callee, args } if callee.as_str() == "Bounds3" => {
                let min = args.iter().find_map(|arg| match arg {
                    SceneArgExpr::Named { name, value } if name.as_str() == "min" => {
                        Some(value.clone())
                    }
                    _ => None,
                })?;
                let max = args.iter().find_map(|arg| match arg {
                    SceneArgExpr::Named { name, value } if name.as_str() == "max" => {
                        Some(value.clone())
                    }
                    _ => None,
                })?;
                Some(SupportExpr::Aabb { min, max })
            }
            _ => None,
        })
        .unwrap_or(SupportExpr::Unknown)
}

fn lower_support_expr(
    expr: &hir::FieldExpr,
    field_body: Option<&hir::Body>,
    metadata: Option<&hir::FieldMetadata>,
) -> SupportExpr {
    match expr {
        hir::FieldExpr::Use { target } => SupportExpr::Use {
            target: target.clone(),
        },
        hir::FieldExpr::Primitive { primitive, args } => {
            lower_primitive_support_expr(*primitive, args, field_body)
        }
        hir::FieldExpr::Union { items } => SupportExpr::Union {
            items: items
                .iter()
                .map(|item| lower_support_expr(item, field_body, metadata))
                .collect(),
        },
        hir::FieldExpr::Intersection { items } => SupportExpr::Intersection {
            items: items
                .iter()
                .map(|item| lower_support_expr(item, field_body, metadata))
                .collect(),
        },
        hir::FieldExpr::Subtract { left, right } => SupportExpr::Difference {
            left: Box::new(lower_support_expr(left, field_body, metadata)),
            right: Box::new(lower_support_expr(right, field_body, metadata)),
        },
        hir::FieldExpr::Translate { translate, body } => SupportExpr::Transform {
            kind: TransformKind::Translate,
            param: lower_scene_body_expr(translate),
            inner: Box::new(lower_support_expr(body, field_body, metadata)),
        },
        hir::FieldExpr::Rotate { rotate, body } => SupportExpr::Transform {
            kind: TransformKind::Rotate,
            param: lower_scene_body_expr(rotate),
            inner: Box::new(lower_support_expr(body, field_body, metadata)),
        },
        hir::FieldExpr::UniformScale { scale, body } => SupportExpr::Transform {
            kind: TransformKind::UniformScale,
            param: lower_scene_body_expr(scale),
            inner: Box::new(lower_support_expr(body, field_body, metadata)),
        },
        hir::FieldExpr::AffineTransform { transform, body } => SupportExpr::Transform {
            kind: TransformKind::AffineTransform,
            param: lower_scene_body_expr(transform),
            inner: Box::new(lower_support_expr(body, field_body, metadata)),
        },
        hir::FieldExpr::Warp { warp, body } => SupportExpr::Transform {
            kind: TransformKind::Warp,
            param: lower_scene_body_expr(warp),
            inner: Box::new(lower_support_expr(body, field_body, metadata)),
        },
        hir::FieldExpr::RepeatLinear { repeat, body } => SupportExpr::Periodic {
            kind: RepeatKind::RepeatLinear,
            period: lower_scene_body_expr(repeat),
            inner: Box::new(lower_support_expr(body, field_body, metadata)),
        },
        hir::FieldExpr::RepeatGrid { repeat, body } => SupportExpr::Periodic {
            kind: RepeatKind::RepeatGrid,
            period: lower_scene_body_expr(repeat),
            inner: Box::new(lower_support_expr(body, field_body, metadata)),
        },
        hir::FieldExpr::RadialRepeat { radial, body } => SupportExpr::Periodic {
            kind: RepeatKind::RadialRepeat,
            period: lower_scene_body_expr(radial),
            inner: Box::new(lower_support_expr(body, field_body, metadata)),
        },
        hir::FieldExpr::MirrorArray { mirror, body } => SupportExpr::Repeat {
            kind: RepeatKind::MirrorArray,
            param: lower_scene_body_expr(mirror),
            inner: Box::new(lower_support_expr(body, field_body, metadata)),
        },
        hir::FieldExpr::InstanceArray { instance, body } => SupportExpr::Repeat {
            kind: RepeatKind::InstanceArray,
            param: lower_scene_body_expr(instance),
            inner: Box::new(lower_support_expr(body, field_body, metadata)),
        },
        hir::FieldExpr::SmoothUnion { items, .. } => SupportExpr::Union {
            items: items
                .iter()
                .map(|item| lower_support_expr(item, field_body, metadata))
                .collect(),
        },
        hir::FieldExpr::SmoothIntersection { items, .. } => SupportExpr::Intersection {
            items: items
                .iter()
                .map(|item| lower_support_expr(item, field_body, metadata))
                .collect(),
        },
        hir::FieldExpr::SmoothSubtract { left, right, .. } => SupportExpr::Difference {
            left: Box::new(lower_support_expr(left, field_body, metadata)),
            right: Box::new(lower_support_expr(right, field_body, metadata)),
        },
        hir::FieldExpr::Bend { bend, body } => SupportExpr::Transform {
            kind: TransformKind::Bend,
            param: lower_scene_body_expr(bend),
            inner: Box::new(lower_support_expr(body, field_body, metadata)),
        },
        hir::FieldExpr::Twist { twist, body } => SupportExpr::Transform {
            kind: TransformKind::Twist,
            param: lower_scene_body_expr(twist),
            inner: Box::new(lower_support_expr(body, field_body, metadata)),
        },
        hir::FieldExpr::Taper { taper, body } => SupportExpr::Transform {
            kind: TransformKind::Taper,
            param: lower_scene_body_expr(taper),
            inner: Box::new(lower_support_expr(body, field_body, metadata)),
        },
        hir::FieldExpr::Displace { displace, body } => SupportExpr::Transform {
            kind: TransformKind::Displace,
            param: lower_scene_body_expr(displace),
            inner: Box::new(lower_support_expr(body, field_body, metadata)),
        },
        hir::FieldExpr::Extrude { .. }
        | hir::FieldExpr::Revolve { .. }
        | hir::FieldExpr::Sweep { .. }
        | hir::FieldExpr::Loft { .. } => lower_profile_support_expr(metadata),
        hir::FieldExpr::Custom { .. } => SupportExpr::OpaqueBoundary {
            bounds: metadata.and_then(lower_authored_bounds_expr),
        },
    }
}

fn lower_shape_node(
    expr: &hir::ShapeExpr,
    fields: &BTreeMap<SmolStr, FieldScene>,
    next_leaf_id: &mut u32,
    leaves: &mut BTreeMap<ShapeLeafId, ShapeLeafScene>,
) -> ShapeNode {
    match expr {
        hir::ShapeExpr::Use { target } => ShapeNode::Use {
            target: target.clone(),
        },
        hir::ShapeExpr::Union { items, .. } => ShapeNode::Union {
            items: items
                .iter()
                .map(|item| lower_shape_node(item, fields, next_leaf_id, leaves))
                .collect(),
        },
        hir::ShapeExpr::Intersection { items, .. } => ShapeNode::Intersection {
            items: items
                .iter()
                .map(|item| lower_shape_node(item, fields, next_leaf_id, leaves))
                .collect(),
        },
        hir::ShapeExpr::Subtract { left, right, .. } => ShapeNode::Subtract {
            left: Box::new(lower_shape_node(left, fields, next_leaf_id, leaves)),
            right: Box::new(lower_shape_node(right, fields, next_leaf_id, leaves)),
        },
        hir::ShapeExpr::Leaf(leaf) => {
            let field_scene = fields.get(&leaf.field);
            let id = ShapeLeafId(*next_leaf_id);
            *next_leaf_id += 1;
            let lowered = ShapeLeafScene {
                id,
                field: leaf.field.clone(),
                material: leaf.material.clone(),
                radiance: leaf.radiance.clone(),
                volume: leaf.volume.clone(),
                payload: leaf.payload.clone(),
                feature_id: leaf.feature_id,
                field_semantics: field_scene
                    .map(|field| field.semantics)
                    .unwrap_or(DistanceSemantics::UnknownOpaque),
                opaque_boundary: field_scene
                    .map(|field| field.opaque_boundary)
                    .unwrap_or(true),
            };
            leaves.insert(id, lowered.clone());
            ShapeNode::Leaf(lowered)
        }
    }
}

fn lower_shape_support_expr(
    expr: &hir::ShapeExpr,
    shape_graphs: &BTreeMap<SmolStr, hir::ShapeGraph>,
    fields: &BTreeMap<SmolStr, FieldScene>,
    cache: &mut BTreeMap<SmolStr, SupportExpr>,
    visiting: &mut BTreeSet<SmolStr>,
) -> SupportExpr {
    match expr {
        hir::ShapeExpr::Use { target } => {
            if let Some(cached) = cache.get(target).cloned() {
                return cached;
            }
            if !visiting.insert(target.clone()) {
                return SupportExpr::Unknown;
            }
            let lowered = shape_graphs
                .get(target)
                .map(|graph| {
                    lower_shape_support_expr(&graph.root, shape_graphs, fields, cache, visiting)
                })
                .unwrap_or_else(|| SupportExpr::Use {
                    target: target.clone(),
                });
            visiting.remove(target);
            cache.insert(target.clone(), lowered.clone());
            lowered
        }
        hir::ShapeExpr::Leaf(leaf) => fields
            .get(&leaf.field)
            .map(|field| field.support_expr.clone())
            .unwrap_or(SupportExpr::Unknown),
        hir::ShapeExpr::Union { items, .. } => SupportExpr::Union {
            items: items
                .iter()
                .map(|item| lower_shape_support_expr(item, shape_graphs, fields, cache, visiting))
                .collect(),
        },
        hir::ShapeExpr::Intersection { items, .. } => SupportExpr::Intersection {
            items: items
                .iter()
                .map(|item| lower_shape_support_expr(item, shape_graphs, fields, cache, visiting))
                .collect(),
        },
        hir::ShapeExpr::Subtract { left, right, .. } => SupportExpr::Difference {
            left: Box::new(lower_shape_support_expr(
                left,
                shape_graphs,
                fields,
                cache,
                visiting,
            )),
            right: Box::new(lower_shape_support_expr(
                right,
                shape_graphs,
                fields,
                cache,
                visiting,
            )),
        },
    }
}

fn lower_shape_provenance_expr(expr: &hir::ShapeProvenanceExpr) -> ShapeProvenanceExpr {
    match expr {
        hir::ShapeProvenanceExpr::Use { target } => ShapeProvenanceExpr::Use {
            target: target.clone(),
        },
        hir::ShapeProvenanceExpr::Union { provenance, items } => ShapeProvenanceExpr::Union {
            provenance: (*provenance).into(),
            items: items.iter().map(lower_shape_provenance_expr).collect(),
        },
        hir::ShapeProvenanceExpr::Intersection { provenance, items } => {
            ShapeProvenanceExpr::Intersection {
                provenance: (*provenance).into(),
                items: items.iter().map(lower_shape_provenance_expr).collect(),
            }
        }
        hir::ShapeProvenanceExpr::Subtract {
            provenance,
            left,
            right,
        } => ShapeProvenanceExpr::Subtract {
            provenance: (*provenance).into(),
            left: Box::new(lower_shape_provenance_expr(left)),
            right: Box::new(lower_shape_provenance_expr(right)),
        },
        hir::ShapeProvenanceExpr::Leaf => ShapeProvenanceExpr::Leaf,
    }
}

fn shape_expr_contains_opaque(
    expr: &hir::ShapeExpr,
    shape_graphs: &BTreeMap<SmolStr, hir::ShapeGraph>,
    fields: &BTreeMap<SmolStr, FieldScene>,
) -> bool {
    match expr {
        hir::ShapeExpr::Use { target } => shape_graphs
            .get(target)
            .map(|graph| shape_expr_contains_opaque(&graph.root, shape_graphs, fields))
            .unwrap_or(false),
        hir::ShapeExpr::Leaf(leaf) => fields
            .get(&leaf.field)
            .map(|field| field.opaque_boundary)
            .unwrap_or(true),
        hir::ShapeExpr::Union { items, .. } | hir::ShapeExpr::Intersection { items, .. } => items
            .iter()
            .any(|item| shape_expr_contains_opaque(item, shape_graphs, fields)),
        hir::ShapeExpr::Subtract { left, right, .. } => {
            shape_expr_contains_opaque(left, shape_graphs, fields)
                || shape_expr_contains_opaque(right, shape_graphs, fields)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::lower as hir_lower;
    use crate::parser::ast;
    use crate::parser::ast::AstNode;
    use crate::parser::parse;

    fn lower_inline_module_from_source(source: &str) -> hir::Module {
        let node = parse(source);
        let root = ast::Root::cast(node).expect("root");
        hir_lower::lower(root)
    }

    #[test]
    fn lowers_semantic_and_opaque_fields_into_scene_ir() {
        let source = r#"
field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

field conservative distance opaque_field(p: Vec3) -> F32 {
    support = Support3(bounds=Bounds3(
        min=vec3(-1.0, -1.0, -1.0),
        max=vec3(1.0, 1.0, 1.0)
    ))
    bounds = Bounds3(
        min=vec3(-1.0, -1.0, -1.0),
        max=vec3(1.0, 1.0, 1.0)
    )
    return length(p - vec3(3.0, 0.0, 0.0)) - 0.5
}
"#;
        let module = lower_inline_module_from_source(source);
        let scene = lower_module(&module);

        let sphere = scene.fields.get("sphere_field").expect("sphere scene");
        assert_eq!(sphere.semantics, DistanceSemantics::ExactSignedDistance);
        assert_eq!(sphere.support_class, SupportClass::Bounded);
        assert!(!sphere.opaque_boundary);
        match &sphere.root {
            FieldNode::Primitive { primitive, .. } => {
                assert_eq!(*primitive, hir::FieldPrimitive::Sphere)
            }
            other => panic!("expected sphere primitive, got {other:?}"),
        }

        let opaque = scene.fields.get("opaque_field").expect("opaque scene");
        assert_eq!(opaque.semantics, DistanceSemantics::UnknownOpaque);
        assert!(opaque.opaque_boundary);
        assert!(!opaque.can_coarse_support_pruning);
        assert!(opaque.support_expr.contains_opaque_leaf());
        match opaque.root {
            FieldNode::OpaqueLeaf => {}
            ref other => panic!("expected opaque leaf, got {other:?}"),
        }
    }

    #[test]
    fn propagates_opaque_boundary_into_shape_scene() {
        let source = r#"
field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

field conservative distance opaque_field(p: Vec3) -> F32 {
    support = Support3(bounds=Bounds3(
        min=vec3(-1.0, -1.0, -1.0),
        max=vec3(1.0, 1.0, 1.0)
    ))
    bounds = Bounds3(
        min=vec3(-1.0, -1.0, -1.0),
        max=vec3(1.0, 1.0, 1.0)
    )
    return length(p - vec3(3.0, 0.0, 0.0)) - 0.5
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.25, 0.35, 0.45),
        roughness=0.5,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape sphere_shape {
    field = sphere_field
    material = shade
    payload = Payload(entity_id=u32(1), material_id=u32(1), actor=ActorHandle(id=u32(1), generation=u32(0)))
}

shape opaque_shape {
    field = opaque_field
    material = shade
    payload = Payload(entity_id=u32(2), material_id=u32(2), actor=ActorHandle(id=u32(2), generation=u32(0)))
}

shape scene_shape {
    union {
        provenance_policy = nearest
        use sphere_shape
        use opaque_shape
    }
}
"#;
        let module = lower_inline_module_from_source(source);
        let scene = lower_module(&module);
        let shape = scene.shapes.get("scene_shape").expect("shape scene");
        assert_eq!(shape.semantics, DistanceSemantics::UnknownOpaque);
        assert!(shape.opaque_boundary);
        assert!(shape.support_expr.contains_opaque_leaf());
    }

    #[test]
    fn leaf_shape_inherits_opaque_quarantine_from_custom_field() {
        let source = r#"
field conservative distance opaque_field(p: Vec3) -> F32 {
    support = Support3(bounds=Bounds3(
        min=vec3(-1.0, -1.0, -1.0),
        max=vec3(1.0, 1.0, 1.0)
    ))
    bounds = Bounds3(
        min=vec3(-1.0, -1.0, -1.0),
        max=vec3(1.0, 1.0, 1.0)
    )
    return length(p - vec3(3.0, 0.0, 0.0)) - 0.5
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.25, 0.35, 0.45),
        roughness=0.5,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape opaque_shape {
    field = opaque_field
    material = shade
    payload = Payload(entity_id=u32(2), material_id=u32(2), actor=ActorHandle(id=u32(2), generation=u32(0)))
}
"#;
        let module = lower_inline_module_from_source(source);
        let scene = lower_module(&module);
        let shape = scene.shapes.get("opaque_shape").expect("shape scene");
        assert_eq!(shape.semantics, DistanceSemantics::UnknownOpaque);
        assert!(shape.opaque_boundary);
        assert!(!shape.can_coarse_support_pruning);
        assert!(shape.support_expr.contains_opaque_leaf());
    }

    #[test]
    fn lowers_repeat_and_boolean_field_structure_into_scene_ir() {
        let source = r#"
field conservative distance repeated_union(p: Vec3) -> F32 {
    union {
        sphere(radius = 1.0)
        repeat_grid = vec3(2.0, 0.0, 0.0) {
            box(half_size = vec3(0.5, 0.5, 0.5))
        }
    }
}
"#;
        let module = lower_inline_module_from_source(source);
        let scene = lower_module(&module);
        let field = scene.fields.get("repeated_union").expect("repeated field");
        assert_eq!(field.semantics, DistanceSemantics::ConservativeLowerBound);
        assert_eq!(field.support_class, SupportClass::Periodic);
        assert!(!field.opaque_boundary);
        match &field.root {
            FieldNode::Union { items } => {
                assert_eq!(items.len(), 2);
                assert!(matches!(&items[0], FieldNode::Primitive { .. }));
                assert!(matches!(
                    &items[1],
                    FieldNode::Repeat {
                        kind: RepeatKind::RepeatGrid,
                        ..
                    }
                ));
            }
            other => panic!("expected repeated union field, got {other:?}"),
        }
    }

    #[test]
    fn lowers_intersection_subtract_and_transform_support_families_into_scene_ir() {
        let source = r#"
field conservative distance base_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

field conservative distance intersection_field(p: Vec3) -> F32 {
    intersection {
        use base_field
        translate = vec3(0.5, 0.0, 0.0) {
            box(half_size = vec3(0.25, 0.25, 0.25))
        }
    }
}

field conservative distance subtract_field(p: Vec3) -> F32 {
    subtract {
        use base_field
        warp = vec3(0.05, 0.0, 0.0) {
            box(half_size = vec3(0.25, 0.25, 0.25))
        }
    }
}

field conservative distance transform_family_field(p: Vec3) -> F32 {
    affine_transform = vec3(0.32, 0.0, 0.0) {
        use base_field
    }
}
"#;
        let module = lower_inline_module_from_source(source);
        let scene = lower_module(&module);

        let intersection = scene
            .fields
            .get("intersection_field")
            .expect("intersection field");
        assert_eq!(intersection.support_class, SupportClass::Bounded);
        match (&intersection.root, &intersection.support_expr) {
            (
                FieldNode::Intersection { items },
                SupportExpr::Intersection {
                    items: support_items,
                },
            ) => {
                assert_eq!(items.len(), 2);
                assert_eq!(support_items.len(), 2);
                assert!(matches!(
                    &support_items[0],
                    SupportExpr::Use {
                        target: name
                    } if name.as_str() == "base_field"
                ));
                assert!(matches!(
                    &support_items[1],
                    SupportExpr::Transform {
                        kind: TransformKind::Translate,
                        ..
                    }
                ));
            }
            other => panic!("expected intersection structure, got {other:?}"),
        }

        let subtract = scene.fields.get("subtract_field").expect("subtract field");
        assert_eq!(subtract.support_class, SupportClass::Bounded);
        match (&subtract.root, &subtract.support_expr) {
            (FieldNode::Subtract { .. }, SupportExpr::Difference { left, right }) => {
                assert!(matches!(
                    left.as_ref(),
                    SupportExpr::Use {
                        target: name
                    } if name.as_str() == "base_field"
                ));
                assert!(matches!(
                    right.as_ref(),
                    SupportExpr::Transform {
                        kind: TransformKind::Warp,
                        ..
                    }
                ));
            }
            other => panic!("expected subtract support structure, got {other:?}"),
        }

        let transformed = scene
            .fields
            .get("transform_family_field")
            .expect("transform family field");
        assert_eq!(transformed.support_class, SupportClass::Bounded);
        match &transformed.support_expr {
            SupportExpr::Transform {
                kind: TransformKind::AffineTransform,
                inner,
                ..
            } => assert!(matches!(
                inner.as_ref(),
                SupportExpr::Use {
                    target: name
                } if name.as_str() == "base_field"
            )),
            other => panic!("expected affine transform support node, got {other:?}"),
        }
    }

    #[test]
    fn lowers_repeat_families_into_support_expressions_and_capabilities() {
        let source = r#"
field exact distance base_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

field conservative distance repeat_linear_field(p: Vec3) -> F32 {
    repeat_linear = vec3(2.0, 0.0, 0.0) {
        use base_field
    }
}

field conservative distance radial_repeat_field(p: Vec3) -> F32 {
    radial_repeat = vec3(0.0, 1.0, 0.0) {
        use base_field
    }
}

field conservative distance mirror_array_field(p: Vec3) -> F32 {
    mirror_array = vec3(1.0, 0.0, 0.0) {
        use base_field
    }
}

field conservative distance instance_array_field(p: Vec3) -> F32 {
    instance_array = Transform3(
        matrix=mat4_identity(),
        inverse=mat4_identity()
    ) {
        use base_field
    }
}
"#;
        let module = lower_inline_module_from_source(source);
        let scene = lower_module(&module);

        let repeat_linear = scene
            .fields
            .get("repeat_linear_field")
            .expect("repeat linear field");
        assert_eq!(repeat_linear.support_class, SupportClass::Periodic);
        match &repeat_linear.support_expr {
            SupportExpr::Periodic {
                kind: RepeatKind::RepeatLinear,
                inner,
                ..
            } => assert!(matches!(
                inner.as_ref(),
                SupportExpr::Use {
                    target: name
                } if name.as_str() == "base_field"
            )),
            other => panic!("expected repeat_linear support node, got {other:?}"),
        }

        let radial_repeat = scene
            .fields
            .get("radial_repeat_field")
            .expect("radial repeat field");
        assert_eq!(radial_repeat.support_class, SupportClass::Periodic);
        match &radial_repeat.support_expr {
            SupportExpr::Periodic {
                kind: RepeatKind::RadialRepeat,
                inner,
                ..
            } => assert!(matches!(
                inner.as_ref(),
                SupportExpr::Use {
                    target: name
                } if name.as_str() == "base_field"
            )),
            other => panic!("expected radial repeat support node, got {other:?}"),
        }

        let mirror_array = scene
            .fields
            .get("mirror_array_field")
            .expect("mirror array field");
        assert_eq!(mirror_array.support_class, SupportClass::Bounded);
        assert!(mirror_array.can_coarse_support_pruning);
        match &mirror_array.support_expr {
            SupportExpr::Repeat {
                kind: RepeatKind::MirrorArray,
                inner,
                ..
            } => assert!(matches!(
                inner.as_ref(),
                SupportExpr::Use {
                    target: name
                } if name.as_str() == "base_field"
            )),
            other => panic!("expected mirror array support node, got {other:?}"),
        }

        let instance_array = scene
            .fields
            .get("instance_array_field")
            .expect("instance array field");
        assert_eq!(instance_array.support_class, SupportClass::Bounded);
        assert!(instance_array.can_coarse_support_pruning);
        match &instance_array.support_expr {
            SupportExpr::Repeat {
                kind: RepeatKind::InstanceArray,
                inner,
                ..
            } => assert!(matches!(
                inner.as_ref(),
                SupportExpr::Use {
                    target: name
                } if name.as_str() == "base_field"
            )),
            other => panic!("expected instance array support node, got {other:?}"),
        }
    }

    #[test]
    fn propagates_shape_support_expressions_through_boolean_composition() {
        let source = r#"
field exact distance left_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

field conservative distance right_field(p: Vec3) -> F32 {
    translate = vec3(0.6, 0.0, 0.0) {
        box(half_size = vec3(0.25, 0.25, 0.25))
    }
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.2, 0.4, 0.6),
        roughness=0.5,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape left_shape {
    field = left_field
    material = shade
    payload = Payload(entity_id=u32(1), material_id=u32(1), actor=ActorHandle(id=u32(1), generation=u32(0)))
}

shape right_shape {
    field = right_field
    material = shade
    payload = Payload(entity_id=u32(2), material_id=u32(2), actor=ActorHandle(id=u32(2), generation=u32(0)))
}

shape union_shape {
    union {
        provenance_policy = nearest
        use left_shape
        use right_shape
    }
}

shape intersection_shape {
    intersection {
        provenance_policy = nearest
        use left_shape
        use right_shape
    }
}

shape subtract_shape {
    subtract {
        provenance_policy = nearest
        use left_shape
        use right_shape
    }
}
"#;
        let module = lower_inline_module_from_source(source);
        let scene = lower_module(&module);

        let union_shape = scene.shapes.get("union_shape").expect("union shape");
        match &union_shape.support_expr {
            SupportExpr::Union { items } => {
                assert_eq!(items.len(), 2);
                assert!(matches!(&items[0], SupportExpr::Sphere { .. }));
                assert!(matches!(
                    &items[1],
                    SupportExpr::Transform {
                        kind: TransformKind::Translate,
                        inner,
                        ..
                    } if matches!(
                        inner.as_ref(),
                        SupportExpr::Aabb { .. }
                    )
                ));
            }
            other => panic!("expected union shape support expr, got {other:?}"),
        }

        let intersection_shape = scene
            .shapes
            .get("intersection_shape")
            .expect("intersection shape");
        match &intersection_shape.support_expr {
            SupportExpr::Intersection { items } => {
                assert_eq!(items.len(), 2);
                assert!(matches!(&items[0], SupportExpr::Sphere { .. }));
                assert!(matches!(
                    &items[1],
                    SupportExpr::Transform {
                        kind: TransformKind::Translate,
                        inner,
                        ..
                    } if matches!(
                        inner.as_ref(),
                        SupportExpr::Aabb { .. }
                    )
                ));
            }
            other => panic!("expected intersection shape support expr, got {other:?}"),
        }

        let subtract_shape = scene.shapes.get("subtract_shape").expect("subtract shape");
        match &subtract_shape.support_expr {
            SupportExpr::Difference { left, right } => {
                assert!(matches!(left.as_ref(), SupportExpr::Sphere { .. }));
                assert!(matches!(
                    right.as_ref(),
                    SupportExpr::Transform {
                        kind: TransformKind::Translate,
                        inner,
                        ..
                    } if matches!(
                        inner.as_ref(),
                        SupportExpr::Aabb { .. }
                    )
                ));
            }
            other => panic!("expected subtract shape support expr, got {other:?}"),
        }
    }

    #[test]
    fn lowers_structured_support_expression_for_wrapped_union() {
        let source = r#"
field conservative distance wrapped_support_field(p: Vec3) -> F32 {
    union {
        sphere(radius = 1.0)
        repeat_grid = vec3(2.0, 0.0, 0.0) {
            translate = vec3(0.5, 0.0, 0.0) {
                box(half = vec3(0.25, 0.25, 0.25))
            }
        }
    }
}
"#;
        let module = lower_inline_module_from_source(source);
        let scene = lower_module(&module);
        let field = scene
            .fields
            .get("wrapped_support_field")
            .expect("wrapped support field");
        match &field.support_expr {
            SupportExpr::Union { items } => {
                assert_eq!(items.len(), 2);
                assert!(matches!(&items[0], SupportExpr::Sphere { .. }));
                match &items[1] {
                    SupportExpr::Periodic {
                        kind: RepeatKind::RepeatGrid,
                        inner,
                        ..
                    } => match inner.as_ref() {
                        SupportExpr::Transform {
                            kind: TransformKind::Translate,
                            inner,
                            ..
                        } => assert!(matches!(inner.as_ref(), SupportExpr::Aabb { .. })),
                        other => panic!("expected translate support node, got {other:?}"),
                    },
                    other => panic!("expected repeat support node, got {other:?}"),
                }
            }
            other => panic!("expected union support expression, got {other:?}"),
        }
    }

    #[test]
    fn lowers_shape_provenance_policies_into_scene_ir() {
        let source = r#"
field exact distance near_field(p: Vec3) -> F32 {
    sphere(radius = 0.6)
}

field exact distance far_field(p: Vec3) -> F32 {
    translate = vec3(0.0, 0.0, -0.35) {
        sphere(radius = 0.8)
    }
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.2, 0.4, 0.6),
        roughness=0.0,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape near_shape {
    field = near_field
    material = shade
    payload = Payload()
}

shape far_shape {
    field = far_field
    material = shade
    payload = Payload()
}

shape ordered_scene {
    union {
        provenance_policy = ordered
        use far_shape
        use near_shape
    }
}

shape carved_scene {
    subtract {
        provenance_policy = right
        use near_shape
        use far_shape
    }
}
"#;
        let module = lower_inline_module_from_source(source);
        let scene = lower_module(&module);

        let ordered_scene = scene.shapes.get("ordered_scene").expect("ordered scene");
        match ordered_scene
            .provenance
            .as_ref()
            .expect("ordered provenance")
        {
            ShapeProvenanceExpr::Union { provenance, items } => {
                assert_eq!(*provenance, ShapeMergeProvenancePolicy::Ordered);
                assert_eq!(items.len(), 2);
                assert!(matches!(
                    &items[0],
                    ShapeProvenanceExpr::Use { target } if target.as_str() == "far_shape"
                ));
                assert!(matches!(
                    &items[1],
                    ShapeProvenanceExpr::Use { target } if target.as_str() == "near_shape"
                ));
            }
            other => panic!("expected ordered union provenance, got {other:?}"),
        }

        let carved_scene = scene.shapes.get("carved_scene").expect("carved scene");
        match carved_scene.provenance.as_ref().expect("carved provenance") {
            ShapeProvenanceExpr::Subtract {
                provenance,
                left,
                right,
            } => {
                assert_eq!(*provenance, ShapeSubtractProvenancePolicy::Right);
                assert!(matches!(
                    left.as_ref(),
                    ShapeProvenanceExpr::Use { target } if target.as_str() == "near_shape"
                ));
                assert!(matches!(
                    right.as_ref(),
                    ShapeProvenanceExpr::Use { target } if target.as_str() == "far_shape"
                ));
            }
            other => panic!("expected subtract provenance, got {other:?}"),
        }
    }

    #[test]
    fn scene_ir_lowering_is_deterministic() {
        let source = r#"
field exact distance repeated_field(p: Vec3) -> F32 {
    repeat_grid = vec3(2.0, 0.0, 0.0) {
        sphere(radius = 1.0)
    }
}
"#;
        let left = lower_module(&lower_inline_module_from_source(source));
        let right = lower_module(&lower_inline_module_from_source(source));
        assert_eq!(left, right);
    }
}
