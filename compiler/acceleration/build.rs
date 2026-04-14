use super::{
    AccelerationCacheDescriptor, AccelerationCandidateClass, AccelerationChildSpan,
    AccelerationForest, AccelerationForestContract, AccelerationForestContractKind,
    AccelerationLeafPayload, AccelerationNode, AccelerationNodeKind, AccelerationRejectionClass,
    AccelerationRejectionRecord, BoundDescriptor, BoundDescriptorKind, FallbackExpectation,
    LineageRecord, ObserverUsageSummary, SupportDescriptor, SupportDescriptorKind,
};
use crate::query_exec::context::QueryExecContext;
use crate::query_exec::cpu::{DirectQueryEvaluator, ShapeUnionAccelerationCandidate};
use crate::query_exec::region::RegionExecCase;
use crate::scene_ir::SupportClass;
use smol_str::SmolStr;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SharedAccelerationCatalog {
    pub world_forests: BTreeMap<(SmolStr, i32), AccelerationForest>,
    pub union_forests: BTreeMap<SmolStr, AccelerationForest>,
}

impl SharedAccelerationCatalog {
    pub fn world(&self, capture: &SmolStr, detail: i32) -> Option<&AccelerationForest> {
        self.world_forests.get(&(capture.clone(), detail))
    }

    pub fn union(&self, shape: &SmolStr) -> Option<&AccelerationForest> {
        self.union_forests.get(shape)
    }

    pub fn all_forests(&self) -> Vec<AccelerationForest> {
        self.world_forests
            .values()
            .cloned()
            .chain(self.union_forests.values().cloned())
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct AccelerationForestBuilder {
    contract: AccelerationForestContract,
    nodes: Vec<AccelerationNode>,
    caches: Vec<AccelerationCacheDescriptor>,
    observer_usage: Vec<ObserverUsageSummary>,
    rejection_reasons: Vec<AccelerationRejectionRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SupportBounds {
    min: [f32; 3],
    max: [f32; 3],
}

#[derive(Debug, Clone)]
struct LogicalCandidate {
    stable_order: u32,
    leaf_id: SmolStr,
    bounds: Option<SupportBounds>,
    leaf_payload: AccelerationLeafPayload,
    support: Option<SupportDescriptor>,
    lineage: Vec<LineageRecord>,
    notes: Vec<SmolStr>,
}

#[derive(Debug, Clone)]
struct BuiltSubtree {
    root_id: SmolStr,
    min_order: u32,
}

#[derive(Debug)]
struct ForestBuildState {
    prefix: SmolStr,
    candidate_class: AccelerationCandidateClass,
    next_cluster_id: u32,
    next_child_cursor: u32,
    nodes: Vec<AccelerationNode>,
}

impl AccelerationForestBuilder {
    pub fn new(contract: AccelerationForestContract) -> Self {
        Self {
            contract,
            nodes: Vec::new(),
            caches: Vec::new(),
            observer_usage: Vec::new(),
            rejection_reasons: Vec::new(),
        }
    }

    pub fn push_node(&mut self, node: AccelerationNode) -> &mut Self {
        self.nodes.push(node);
        self
    }

    pub fn push_cache(&mut self, cache: AccelerationCacheDescriptor) -> &mut Self {
        self.caches.push(cache);
        self
    }

    pub fn push_observer_usage(&mut self, usage: ObserverUsageSummary) -> &mut Self {
        self.observer_usage.push(usage);
        self
    }

    pub fn push_rejection(&mut self, rejection: AccelerationRejectionRecord) -> &mut Self {
        self.rejection_reasons.push(rejection);
        self
    }

    pub fn extend_rejections(
        &mut self,
        rejections: impl IntoIterator<Item = AccelerationRejectionRecord>,
    ) -> &mut Self {
        self.rejection_reasons.extend(rejections);
        self
    }

    pub fn finish(self) -> AccelerationForest {
        AccelerationForest::new(self.contract, self.nodes, self.caches, self.observer_usage)
            .with_rejection_reasons(self.rejection_reasons)
    }
}

impl ForestBuildState {
    fn new(prefix: impl Into<SmolStr>, candidate_class: AccelerationCandidateClass) -> Self {
        Self {
            prefix: prefix.into(),
            candidate_class,
            next_cluster_id: 0,
            next_child_cursor: 0,
            nodes: Vec::new(),
        }
    }

    fn push_leaf(&mut self, candidate: &LogicalCandidate) -> BuiltSubtree {
        let mut node = AccelerationNode::new(
            candidate.leaf_id.clone(),
            candidate.stable_order.saturating_mul(2).saturating_add(2),
            AccelerationNodeKind::LeafCandidate,
            self.candidate_class,
        )
        .with_leaf_payload(candidate.leaf_payload.clone());
        if let Some(bounds) = candidate.bounds {
            node.bounds.push(bounds_descriptor(bounds));
        }
        node.support = candidate.support.clone();
        node.lineage = candidate.lineage.clone();
        node.notes = candidate.notes.clone();
        self.nodes.push(node);
        BuiltSubtree {
            root_id: candidate.leaf_id.clone(),
            min_order: candidate.stable_order,
        }
    }

    fn push_cluster(
        &mut self,
        children: Vec<BuiltSubtree>,
        bounds: Option<SupportBounds>,
        note: impl Into<SmolStr>,
    ) -> Option<BuiltSubtree> {
        if children.is_empty() {
            return None;
        }
        let id = SmolStr::new(format!("{}::cluster:{}", self.prefix, self.next_cluster_id));
        self.next_cluster_id += 1;
        let mut node = AccelerationNode::new(
            id.clone(),
            children
                .iter()
                .map(|child| child.min_order)
                .min()
                .unwrap_or_default()
                .saturating_mul(2)
                .saturating_add(1),
            AccelerationNodeKind::UnionCluster,
            self.candidate_class,
        );
        node.child_ids = children.iter().map(|child| child.root_id.clone()).collect();
        node.child_span = Some(AccelerationChildSpan::new(
            self.next_child_cursor,
            node.child_ids.len() as u32,
        ));
        self.next_child_cursor += node.child_ids.len() as u32;
        if let Some(bounds) = bounds {
            node.bounds.push(bounds_descriptor(bounds));
        }
        node.notes = vec![note.into()];
        self.nodes.push(node);
        Some(BuiltSubtree {
            root_id: id,
            min_order: children
                .iter()
                .map(|child| child.min_order)
                .min()
                .unwrap_or_default(),
        })
    }
}

pub fn world_forest_builder(contract: AccelerationForestContract) -> AccelerationForestBuilder {
    assert!(matches!(
        contract.kind,
        AccelerationForestContractKind::SharedAccelerationForest
    ));
    AccelerationForestBuilder::new(contract)
}

pub fn union_subtree_forest_builder(
    contract: AccelerationForestContract,
) -> AccelerationForestBuilder {
    assert!(matches!(
        contract.kind,
        AccelerationForestContractKind::SharedUnionSubtreeForest
    ));
    AccelerationForestBuilder::new(contract)
}

pub fn build_shared_acceleration_forests(ctx: &QueryExecContext) -> SharedAccelerationCatalog {
    let evaluator = DirectQueryEvaluator::new(ctx);
    let mut catalog = SharedAccelerationCatalog::default();

    let mut region_cases = ctx.region_cases.clone();
    region_cases.sort_by(|left, right| {
        left.region_name
            .cmp(&right.region_name)
            .then(left.scene_id.cmp(&right.scene_id))
    });
    for case in &region_cases {
        for detail in [0, 1] {
            if let Some(forest) = build_world_forest(ctx, &evaluator, case, detail) {
                catalog
                    .world_forests
                    .insert((case.region_name.clone(), detail), forest);
            }
        }
    }

    let mut shape_names = ctx.scene.shapes.keys().cloned().collect::<Vec<_>>();
    shape_names.sort();
    for shape in &shape_names {
        if let Some(forest) = build_union_forest(ctx, &evaluator, shape) {
            catalog.union_forests.insert(shape.clone(), forest);
        }
    }

    catalog
}

fn build_world_forest(
    ctx: &QueryExecContext,
    evaluator: &DirectQueryEvaluator<'_>,
    case: &RegionExecCase,
    detail: i32,
) -> Option<AccelerationForest> {
    let detail_label = detail_label(detail);
    let shapes = case.shapes_for_detail(detail).ok()?;
    if shapes.is_empty() {
        return None;
    }
    let root_id = SmolStr::new(format!(
        "shared_acceleration_forest::{}::{}::root",
        case.region_name, detail_label
    ));
    let contract = AccelerationForestContract {
        id: SmolStr::new(format!(
            "shared_acceleration_forest::{}::{}",
            case.region_name, detail_label
        )),
        kind: AccelerationForestContractKind::SharedAccelerationForest,
        forest_version: 1,
        candidate_class: AccelerationCandidateClass::SpatialRay,
        root_nodes: vec![root_id.clone()],
        fallback_expectation: FallbackExpectation::ConservativeOnly,
    };

    let mut candidates = Vec::new();
    let mut rejections = Vec::new();
    for (stable_order, shape) in shapes.iter().enumerate() {
        let scene = ctx.scene.shapes.get(shape)?;
        let bounds = evaluator
            .shape_support_bounds_world(shape)
            .ok()
            .flatten()
            .map(|(min, max)| SupportBounds { min, max });
        if scene.opaque_boundary {
            rejections.push(AccelerationRejectionRecord::new(
                AccelerationRejectionClass::OpaqueBoundary,
                shape.clone(),
                format!(
                    "shape '{}' keeps an opaque support boundary in region '{}'",
                    shape, case.region_name
                ),
            ));
        }
        if bounds.is_none()
            && matches!(
                scene.support_class,
                SupportClass::Unknown | SupportClass::Unbounded | SupportClass::Periodic
            )
        {
            rejections.push(AccelerationRejectionRecord::new(
                AccelerationRejectionClass::UnboundedSupport,
                shape.clone(),
                format!(
                    "shape '{}' cannot contribute bounded world pruning for {} detail",
                    shape, detail_label
                ),
            ));
        }
        candidates.push(LogicalCandidate {
            stable_order: stable_order as u32,
            leaf_id: SmolStr::new(format!(
                "shared_acceleration_forest::{}::{}::leaf:{}",
                case.region_name, detail_label, stable_order
            )),
            bounds,
            leaf_payload: AccelerationLeafPayload::new(
                shape.clone(),
                Some(ctx.shape_root_feature_id(shape).to_string()),
                Some(case.region_name.clone()),
                Some(detail_label),
            ),
            support: Some(support_descriptor(
                shape.clone(),
                scene.support_class,
                scene.opaque_boundary,
                scene.can_coarse_support_pruning,
            )),
            lineage: vec![LineageRecord {
                semantic_id: shape.clone(),
                source_path: case.region_name.clone(),
                stable_order: stable_order as u32,
            }],
            notes: vec![
                SmolStr::new(format!("shape={shape}")),
                SmolStr::new(format!("detail={detail_label}")),
            ],
        });
    }

    Some(build_candidate_forest(
        contract,
        root_id,
        AccelerationCandidateClass::SpatialRay,
        candidates,
        Some(SupportDescriptor {
            kind: SupportDescriptorKind::ConservativeLowerBound,
            semantics: SmolStr::new(format!("world:{}:{}", case.region_name, detail_label)),
            opaque_boundary: false,
            can_coarse_prune: true,
        }),
        vec![
            SmolStr::new(format!("capture={}", case.region_name)),
            SmolStr::new(format!("detail={detail_label}")),
            SmolStr::new(format!("shape_count={}", shapes.len())),
        ],
        rejections,
    ))
}

fn build_union_forest(
    ctx: &QueryExecContext,
    evaluator: &DirectQueryEvaluator<'_>,
    shape: &SmolStr,
) -> Option<AccelerationForest> {
    let items = evaluator
        .shape_root_union_candidate_bounds(shape)
        .ok()
        .flatten()?;
    if items.is_empty() {
        return None;
    }
    let scene = ctx.scene.shapes.get(shape)?;
    let root_id = SmolStr::new(format!("shared_union_subtree_forest::{}::root", shape));
    let contract = AccelerationForestContract {
        id: SmolStr::new(format!("shared_union_subtree_forest::{shape}")),
        kind: AccelerationForestContractKind::SharedUnionSubtreeForest,
        forest_version: 1,
        candidate_class: AccelerationCandidateClass::SpatialPoint,
        root_nodes: vec![root_id.clone()],
        fallback_expectation: FallbackExpectation::ConservativeOnly,
    };

    let mut candidates = Vec::new();
    let mut rejections = Vec::new();
    for candidate in items {
        if candidate.bounds.is_none() {
            rejections.push(AccelerationRejectionRecord::new(
                AccelerationRejectionClass::UnboundedSupport,
                format!("{shape}::{}", candidate.index),
                "union child is missing conservative support bounds",
            ));
        }
        candidates.push(union_candidate(shape, scene, candidate));
    }

    Some(build_candidate_forest(
        contract,
        root_id,
        AccelerationCandidateClass::SpatialPoint,
        candidates,
        Some(support_descriptor(
            shape.clone(),
            scene.support_class,
            scene.opaque_boundary,
            scene.can_coarse_support_pruning,
        )),
        vec![
            SmolStr::new(format!("shape={shape}")),
            SmolStr::new(format!("union_children={}", scene.node_records.len())),
        ],
        rejections,
    ))
}

fn union_candidate(
    shape: &SmolStr,
    scene: &crate::scene_ir::ShapeScene,
    candidate: ShapeUnionAccelerationCandidate,
) -> LogicalCandidate {
    LogicalCandidate {
        stable_order: candidate.index as u32,
        leaf_id: SmolStr::new(format!(
            "shared_union_subtree_forest::{}::leaf:{}",
            shape, candidate.index
        )),
        bounds: candidate
            .bounds
            .map(|(min, max)| SupportBounds { min, max }),
        leaf_payload: AccelerationLeafPayload::new(
            shape.clone(),
            Some(candidate.index.to_string()),
            Some(shape.clone()),
            None::<SmolStr>,
        ),
        support: Some(support_descriptor(
            shape.clone(),
            scene.support_class,
            scene.opaque_boundary,
            scene.can_coarse_support_pruning,
        )),
        lineage: vec![LineageRecord {
            semantic_id: shape.clone(),
            source_path: SmolStr::new(format!("root_union:{}", candidate.index)),
            stable_order: candidate.index as u32,
        }],
        notes: vec![SmolStr::new(format!("union_item={}", candidate.index))],
    }
}

fn build_candidate_forest(
    contract: AccelerationForestContract,
    root_id: SmolStr,
    candidate_class: AccelerationCandidateClass,
    candidates: Vec<LogicalCandidate>,
    root_support: Option<SupportDescriptor>,
    root_notes: Vec<SmolStr>,
    rejections: Vec<AccelerationRejectionRecord>,
) -> AccelerationForest {
    let mut state = ForestBuildState::new(root_id.clone(), candidate_class);
    let subtree = build_candidate_subtree(&mut state, &candidates);
    let mut builder = match contract.kind {
        AccelerationForestContractKind::SharedAccelerationForest => world_forest_builder(contract),
        AccelerationForestContractKind::SharedUnionSubtreeForest => {
            union_subtree_forest_builder(contract)
        }
    };

    if let Some(subtree) = subtree {
        let mut root = AccelerationNode::new(
            root_id,
            0,
            AccelerationNodeKind::ForestRoot,
            candidate_class,
        );
        root.child_ids = vec![subtree.root_id];
        root.child_span = Some(AccelerationChildSpan::new(state.next_child_cursor, 1));
        root.support = root_support;
        if subtree_is_fully_bounded(&candidates)
            && let Some(bounds) = candidates
                .iter()
                .filter_map(|candidate| candidate.bounds)
                .reduce(merge_union_support_bounds)
        {
            root.bounds.push(bounds_descriptor(bounds));
        }
        root.notes = root_notes;
        state.nodes.push(root);
    }

    for node in state.nodes {
        builder.push_node(node);
    }
    builder.extend_rejections(rejections);
    builder.finish()
}

fn build_candidate_subtree(
    state: &mut ForestBuildState,
    candidates: &[LogicalCandidate],
) -> Option<BuiltSubtree> {
    if candidates.is_empty() {
        return None;
    }
    if candidates.len() == 1 {
        return Some(state.push_leaf(&candidates[0]));
    }

    let mut bounded = candidates
        .iter()
        .filter(|candidate| candidate.bounds.is_some())
        .cloned()
        .collect::<Vec<_>>();
    let mut unbounded = candidates
        .iter()
        .filter(|candidate| candidate.bounds.is_none())
        .cloned()
        .collect::<Vec<_>>();
    bounded.sort_by_key(|candidate| candidate.stable_order);
    unbounded.sort_by_key(|candidate| candidate.stable_order);

    let mut children = Vec::new();
    if bounded.len() > 2 {
        let axis = dominant_bounds_axis(&bounded);
        bounded.sort_by(|left, right| {
            support_bounds_center(left.bounds.expect("bounded"), axis)
                .partial_cmp(&support_bounds_center(right.bounds.expect("bounded"), axis))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(left.stable_order.cmp(&right.stable_order))
        });
        let mid = bounded.len() / 2;
        if let Some(left) = build_candidate_subtree(state, &bounded[..mid]) {
            children.push(left);
        }
        if let Some(right) = build_candidate_subtree(state, &bounded[mid..]) {
            children.push(right);
        }
    } else {
        for candidate in &bounded {
            if let Some(child) = build_candidate_subtree(state, std::slice::from_ref(candidate)) {
                children.push(child);
            }
        }
    }

    for candidate in &unbounded {
        if let Some(child) = build_candidate_subtree(state, std::slice::from_ref(candidate)) {
            children.push(child);
        }
    }

    children.sort_by_key(|child| child.min_order);
    state.push_cluster(
        children,
        subtree_is_fully_bounded(candidates).then(|| {
            candidates
                .iter()
                .filter_map(|candidate| candidate.bounds)
                .reduce(merge_union_support_bounds)
                .expect("fully bounded subtree must yield merged bounds")
        }),
        SmolStr::new(format!("leaf_count={}", candidates.len())),
    )
}

fn subtree_is_fully_bounded(candidates: &[LogicalCandidate]) -> bool {
    candidates
        .iter()
        .all(|candidate| candidate.bounds.is_some())
}

fn support_descriptor(
    semantics: impl Into<SmolStr>,
    support_class: SupportClass,
    opaque_boundary: bool,
    can_coarse_prune: bool,
) -> SupportDescriptor {
    SupportDescriptor {
        kind: if opaque_boundary {
            SupportDescriptorKind::OpaqueBoundary
        } else if matches!(support_class, SupportClass::Bounded) {
            SupportDescriptorKind::ExactIntervalBound
        } else {
            SupportDescriptorKind::ConservativeLowerBound
        },
        semantics: semantics.into(),
        opaque_boundary,
        can_coarse_prune,
    }
}

fn bounds_descriptor(bounds: SupportBounds) -> BoundDescriptor {
    BoundDescriptor {
        kind: BoundDescriptorKind::AxisAlignedBounds,
        summary: bounds_summary(bounds),
    }
}

fn bounds_summary(bounds: SupportBounds) -> SmolStr {
    SmolStr::new(format!(
        "min={},{},{}|max={},{},{}",
        bounds.min[0], bounds.min[1], bounds.min[2], bounds.max[0], bounds.max[1], bounds.max[2]
    ))
}

fn detail_label(detail: i32) -> &'static str {
    if detail > 0 { "fine" } else { "coarse" }
}

fn empty_support_bounds() -> SupportBounds {
    SupportBounds {
        min: [f32::INFINITY; 3],
        max: [f32::NEG_INFINITY; 3],
    }
}

fn normalize_support_bounds(bounds: SupportBounds) -> SupportBounds {
    SupportBounds {
        min: [
            bounds.min[0].min(bounds.max[0]),
            bounds.min[1].min(bounds.max[1]),
            bounds.min[2].min(bounds.max[2]),
        ],
        max: [
            bounds.min[0].max(bounds.max[0]),
            bounds.min[1].max(bounds.max[1]),
            bounds.min[2].max(bounds.max[2]),
        ],
    }
}

fn merge_union_support_bounds(lhs: SupportBounds, rhs: SupportBounds) -> SupportBounds {
    if lhs.min[0].is_infinite() {
        return rhs;
    }
    if rhs.min[0].is_infinite() {
        return lhs;
    }
    SupportBounds {
        min: [
            lhs.min[0].min(rhs.min[0]),
            lhs.min[1].min(rhs.min[1]),
            lhs.min[2].min(rhs.min[2]),
        ],
        max: [
            lhs.max[0].max(rhs.max[0]),
            lhs.max[1].max(rhs.max[1]),
            lhs.max[2].max(rhs.max[2]),
        ],
    }
}

fn dominant_bounds_axis(candidates: &[LogicalCandidate]) -> usize {
    let overall = candidates
        .iter()
        .filter_map(|candidate| candidate.bounds)
        .reduce(merge_union_support_bounds)
        .unwrap_or_else(empty_support_bounds);
    let extents = [
        (overall.max[0] - overall.min[0]).abs(),
        (overall.max[1] - overall.min[1]).abs(),
        (overall.max[2] - overall.min[2]).abs(),
    ];
    if extents[1] > extents[0] && extents[1] >= extents[2] {
        1
    } else if extents[2] > extents[0] && extents[2] > extents[1] {
        2
    } else {
        0
    }
}

fn support_bounds_center(bounds: SupportBounds, axis: usize) -> f32 {
    let bounds = normalize_support_bounds(bounds);
    (bounds.min[axis] + bounds.max[axis]) * 0.5
}
