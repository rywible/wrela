use crate::hir;
use smol_str::SmolStr;
use std::collections::{BTreeMap, BTreeSet};

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
    Bounded,
    Periodic,
    Unbounded,
    Use {
        target: SmolStr,
    },
    Primitive {
        primitive: hir::FieldPrimitive,
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
        inner: Box<SupportExpr>,
    },
    Repeat {
        kind: RepeatKind,
        inner: Box<SupportExpr>,
    },
    Smooth {
        kind: SmoothKind,
        items: Vec<SupportExpr>,
    },
    ProfileOp {
        kind: ProfileOpKind,
    },
    OpaqueLeaf,
}

impl SupportExpr {
    pub fn contains_opaque_leaf(&self) -> bool {
        match self {
            SupportExpr::OpaqueLeaf => true,
            SupportExpr::Union { items }
            | SupportExpr::Intersection { items }
            | SupportExpr::Smooth { items, .. } => {
                items.iter().any(SupportExpr::contains_opaque_leaf)
            }
            SupportExpr::Difference { left, right } => {
                left.contains_opaque_leaf() || right.contains_opaque_leaf()
            }
            SupportExpr::Transform { inner, .. } | SupportExpr::Repeat { inner, .. } => {
                inner.contains_opaque_leaf()
            }
            SupportExpr::Unknown
            | SupportExpr::Bounded
            | SupportExpr::Periodic
            | SupportExpr::Unbounded
            | SupportExpr::Use { .. }
            | SupportExpr::Primitive { .. }
            | SupportExpr::ProfileOp { .. } => false,
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
    pub semantics: DistanceSemantics,
    pub support_class: SupportClass,
    pub support_expr: SupportExpr,
    pub authored_bounds: Option<SceneValueExpr>,
    pub opaque_boundary: bool,
    pub can_coarse_support_pruning: bool,
    pub trace: hir::GraphTraceMetadata,
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
    pub field: SmolStr,
    pub material: SmolStr,
    pub radiance: Option<SmolStr>,
    pub volume: Option<SmolStr>,
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
    pub support_expr: SupportExpr,
    pub semantics: DistanceSemantics,
    pub support_class: SupportClass,
    pub opaque_boundary: bool,
    pub can_coarse_support_pruning: bool,
    pub trace: hir::GraphTraceMetadata,
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

pub fn lower_module(module: &hir::Module) -> SceneIrModule {
    SceneIrModule::from_hir(module)
}

pub type SceneModule = SceneIrModule;

#[derive(Debug, Clone)]
struct FieldSceneDraft {
    root: FieldNode,
    support_expr: SupportExpr,
    authored_bounds: Option<SceneValueExpr>,
    trace: hir::GraphTraceMetadata,
    declared_class: hir::FieldClass,
    authored_bounded: bool,
}

#[derive(Debug, Clone)]
struct ShapeSceneDraft {
    root: ShapeNode,
    support_expr: SupportExpr,
    trace: hir::GraphTraceMetadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CapabilitySummary {
    semantics: DistanceSemantics,
    support_class: SupportClass,
    opaque_boundary: bool,
    can_coarse_support_pruning: bool,
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
            (
                name.clone(),
                FieldSceneDraft {
                    root: lower_field_node(&graph.root, field_bodies.get(name)),
                    support_expr: lower_support_expr(&graph.root),
                    authored_bounds: metadata.and_then(lower_authored_bounds_expr),
                    trace: graph.trace,
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
    for (name, graph) in field_graphs {
        let summary = analyze_field_scene(name, &drafts, &mut cache, &mut BTreeSet::new());
        let draft = drafts.get(name).expect("field draft");
        fields.insert(
            name.clone(),
            FieldScene {
                name: name.clone(),
                root: draft.root.clone(),
                semantics: summary.semantics,
                support_class: summary.support_class,
                support_expr: draft.support_expr.clone(),
                authored_bounds: draft.authored_bounds.clone(),
                opaque_boundary: summary.opaque_boundary,
                can_coarse_support_pruning: summary.can_coarse_support_pruning,
                trace: graph.trace,
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
            (
                name.clone(),
                ShapeSceneDraft {
                    root: lower_shape_node(&graph.root, fields),
                    support_expr: lower_shape_support_expr(
                        &graph.root,
                        shape_graphs,
                        fields,
                        &mut support_cache,
                        &mut support_visiting,
                    ),
                    trace: graph.trace,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut cache = BTreeMap::new();
    let mut shapes = BTreeMap::new();
    for (name, graph) in shape_graphs {
        let summary = analyze_shape_scene(name, &drafts, fields, &mut cache, &mut BTreeSet::new());
        let draft = drafts.get(name).expect("shape draft");
        shapes.insert(
            name.clone(),
            ShapeScene {
                name: name.clone(),
                root: draft.root.clone(),
                support_expr: draft.support_expr.clone(),
                semantics: summary.semantics,
                support_class: summary.support_class,
                opaque_boundary: summary.opaque_boundary,
                can_coarse_support_pruning: summary.can_coarse_support_pruning,
                trace: graph.trace,
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

fn lower_support_expr(expr: &hir::FieldExpr) -> SupportExpr {
    match expr {
        hir::FieldExpr::Use { target } => SupportExpr::Use {
            target: target.clone(),
        },
        hir::FieldExpr::Primitive { primitive, .. } => SupportExpr::Primitive {
            primitive: *primitive,
        },
        hir::FieldExpr::Union { items } => SupportExpr::Union {
            items: items.iter().map(lower_support_expr).collect(),
        },
        hir::FieldExpr::Intersection { items } => SupportExpr::Intersection {
            items: items.iter().map(lower_support_expr).collect(),
        },
        hir::FieldExpr::Subtract { left, right } => SupportExpr::Difference {
            left: Box::new(lower_support_expr(left)),
            right: Box::new(lower_support_expr(right)),
        },
        hir::FieldExpr::Translate { body, .. } => SupportExpr::Transform {
            kind: TransformKind::Translate,
            inner: Box::new(lower_support_expr(body)),
        },
        hir::FieldExpr::Rotate { body, .. } => SupportExpr::Transform {
            kind: TransformKind::Rotate,
            inner: Box::new(lower_support_expr(body)),
        },
        hir::FieldExpr::UniformScale { body, .. } => SupportExpr::Transform {
            kind: TransformKind::UniformScale,
            inner: Box::new(lower_support_expr(body)),
        },
        hir::FieldExpr::AffineTransform { body, .. } => SupportExpr::Transform {
            kind: TransformKind::AffineTransform,
            inner: Box::new(lower_support_expr(body)),
        },
        hir::FieldExpr::Warp { body, .. } => SupportExpr::Transform {
            kind: TransformKind::Warp,
            inner: Box::new(lower_support_expr(body)),
        },
        hir::FieldExpr::RepeatLinear { body, .. } => SupportExpr::Repeat {
            kind: RepeatKind::RepeatLinear,
            inner: Box::new(lower_support_expr(body)),
        },
        hir::FieldExpr::RepeatGrid { body, .. } => SupportExpr::Repeat {
            kind: RepeatKind::RepeatGrid,
            inner: Box::new(lower_support_expr(body)),
        },
        hir::FieldExpr::RadialRepeat { body, .. } => SupportExpr::Repeat {
            kind: RepeatKind::RadialRepeat,
            inner: Box::new(lower_support_expr(body)),
        },
        hir::FieldExpr::MirrorArray { body, .. } => SupportExpr::Repeat {
            kind: RepeatKind::MirrorArray,
            inner: Box::new(lower_support_expr(body)),
        },
        hir::FieldExpr::InstanceArray { body, .. } => SupportExpr::Repeat {
            kind: RepeatKind::InstanceArray,
            inner: Box::new(lower_support_expr(body)),
        },
        hir::FieldExpr::SmoothUnion { items, .. } => SupportExpr::Smooth {
            kind: SmoothKind::Union,
            items: items.iter().map(lower_support_expr).collect(),
        },
        hir::FieldExpr::SmoothIntersection { items, .. } => SupportExpr::Smooth {
            kind: SmoothKind::Intersection,
            items: items.iter().map(lower_support_expr).collect(),
        },
        hir::FieldExpr::SmoothSubtract { left, right, .. } => SupportExpr::Smooth {
            kind: SmoothKind::Subtract,
            items: vec![lower_support_expr(left), lower_support_expr(right)],
        },
        hir::FieldExpr::Bend { body, .. } => SupportExpr::Transform {
            kind: TransformKind::Bend,
            inner: Box::new(lower_support_expr(body)),
        },
        hir::FieldExpr::Twist { body, .. } => SupportExpr::Transform {
            kind: TransformKind::Twist,
            inner: Box::new(lower_support_expr(body)),
        },
        hir::FieldExpr::Taper { body, .. } => SupportExpr::Transform {
            kind: TransformKind::Taper,
            inner: Box::new(lower_support_expr(body)),
        },
        hir::FieldExpr::Displace { body, .. } => SupportExpr::Transform {
            kind: TransformKind::Displace,
            inner: Box::new(lower_support_expr(body)),
        },
        hir::FieldExpr::Extrude { .. } => SupportExpr::ProfileOp {
            kind: ProfileOpKind::Extrude,
        },
        hir::FieldExpr::Revolve { .. } => SupportExpr::ProfileOp {
            kind: ProfileOpKind::Revolve,
        },
        hir::FieldExpr::Sweep { .. } => SupportExpr::ProfileOp {
            kind: ProfileOpKind::Sweep,
        },
        hir::FieldExpr::Loft { .. } => SupportExpr::ProfileOp {
            kind: ProfileOpKind::Loft,
        },
        hir::FieldExpr::Custom { .. } => SupportExpr::OpaqueLeaf,
    }
}

fn lower_shape_node(expr: &hir::ShapeExpr, fields: &BTreeMap<SmolStr, FieldScene>) -> ShapeNode {
    match expr {
        hir::ShapeExpr::Use { target } => ShapeNode::Use {
            target: target.clone(),
        },
        hir::ShapeExpr::Union { items, .. } => ShapeNode::Union {
            items: items
                .iter()
                .map(|item| lower_shape_node(item, fields))
                .collect(),
        },
        hir::ShapeExpr::Intersection { items, .. } => ShapeNode::Intersection {
            items: items
                .iter()
                .map(|item| lower_shape_node(item, fields))
                .collect(),
        },
        hir::ShapeExpr::Subtract { left, right, .. } => ShapeNode::Subtract {
            left: Box::new(lower_shape_node(left, fields)),
            right: Box::new(lower_shape_node(right, fields)),
        },
        hir::ShapeExpr::Leaf(leaf) => {
            let field_scene = fields.get(&leaf.field);
            ShapeNode::Leaf(ShapeLeafScene {
                field: leaf.field.clone(),
                material: leaf.material.clone(),
                radiance: leaf.radiance.clone(),
                volume: leaf.volume.clone(),
                feature_id: leaf.feature_id,
                field_semantics: field_scene
                    .map(|field| field.semantics)
                    .unwrap_or(DistanceSemantics::UnknownOpaque),
                opaque_boundary: field_scene
                    .map(|field| field.opaque_boundary)
                    .unwrap_or(true),
            })
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
            SupportExpr::Repeat {
                kind: RepeatKind::RepeatLinear,
                inner,
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
            SupportExpr::Repeat {
                kind: RepeatKind::RadialRepeat,
                inner,
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
                assert!(matches!(
                    &items[0],
                    SupportExpr::Primitive {
                        primitive: hir::FieldPrimitive::Sphere
                    }
                ));
                assert!(matches!(
                    &items[1],
                    SupportExpr::Transform {
                        kind: TransformKind::Translate,
                        inner,
                    } if matches!(
                        inner.as_ref(),
                        SupportExpr::Primitive {
                            primitive: hir::FieldPrimitive::Box
                        }
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
                assert!(matches!(
                    &items[0],
                    SupportExpr::Primitive {
                        primitive: hir::FieldPrimitive::Sphere
                    }
                ));
                assert!(matches!(
                    &items[1],
                    SupportExpr::Transform {
                        kind: TransformKind::Translate,
                        inner,
                    } if matches!(
                        inner.as_ref(),
                        SupportExpr::Primitive {
                            primitive: hir::FieldPrimitive::Box
                        }
                    )
                ));
            }
            other => panic!("expected intersection shape support expr, got {other:?}"),
        }

        let subtract_shape = scene.shapes.get("subtract_shape").expect("subtract shape");
        match &subtract_shape.support_expr {
            SupportExpr::Difference { left, right } => {
                assert!(matches!(
                    left.as_ref(),
                    SupportExpr::Primitive {
                        primitive: hir::FieldPrimitive::Sphere
                    }
                ));
                assert!(matches!(
                    right.as_ref(),
                    SupportExpr::Transform {
                        kind: TransformKind::Translate,
                        inner,
                    } if matches!(
                        inner.as_ref(),
                        SupportExpr::Primitive {
                            primitive: hir::FieldPrimitive::Box
                        }
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
                assert!(matches!(
                    &items[0],
                    SupportExpr::Primitive {
                        primitive: hir::FieldPrimitive::Sphere
                    }
                ));
                match &items[1] {
                    SupportExpr::Repeat {
                        kind: RepeatKind::RepeatGrid,
                        inner,
                    } => match inner.as_ref() {
                        SupportExpr::Transform {
                            kind: TransformKind::Translate,
                            inner,
                        } => assert!(matches!(
                            inner.as_ref(),
                            SupportExpr::Primitive {
                                primitive: hir::FieldPrimitive::Box
                            }
                        )),
                        other => panic!("expected translate support node, got {other:?}"),
                    },
                    other => panic!("expected repeat support node, got {other:?}"),
                }
            }
            other => panic!("expected union support expression, got {other:?}"),
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
