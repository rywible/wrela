use smol_str::SmolStr;
use wrela::acceleration::build::{union_subtree_forest_builder, world_forest_builder};
use wrela::acceleration::cache::CacheDisableReason;
use wrela::acceleration::report::{
    AccelerationChildSpan, AccelerationLeafPayload, AccelerationNode, AccelerationRejectionClass,
    AccelerationRejectionRecord, AccelerationReport,
};
use wrela::acceleration::{
    AccelerationCacheDescriptor, AccelerationCacheKind, AccelerationCandidateClass,
    AccelerationForestContract, AccelerationForestContractKind, AccelerationNodeKind,
    AccelerationObserver, CacheArtifactScope, FallbackExpectation, ObserverUsageSummary,
};
use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::parser::ast;
use wrela::parser::ast::AstNode;
use wrela::parser::parse;
use wrela::query_exec::QueryExecContext;

fn shared_world_contract() -> AccelerationForestContract {
    AccelerationForestContract {
        id: SmolStr::new("world_forest"),
        kind: AccelerationForestContractKind::SharedAccelerationForest,
        forest_version: 1,
        candidate_class: AccelerationCandidateClass::SpatialRay,
        root_nodes: vec![SmolStr::new("root_b"), SmolStr::new("root_a")],
        fallback_expectation: FallbackExpectation::ConservativeOnly,
    }
}

fn union_subtree_contract() -> AccelerationForestContract {
    AccelerationForestContract {
        id: SmolStr::new("union_subtree_forest"),
        kind: AccelerationForestContractKind::SharedUnionSubtreeForest,
        forest_version: 1,
        candidate_class: AccelerationCandidateClass::CollisionRefinement,
        root_nodes: vec![SmolStr::new("union_root")],
        fallback_expectation: FallbackExpectation::ExplicitSemanticWeakening,
    }
}

fn lower_inline_module_from_source(source: &str) -> hir::Module {
    let node = parse(source);
    let root = ast::Root::cast(node).expect("root");
    hir_lower::lower(root)
}

fn typed_query_context(source: &str) -> QueryExecContext {
    let module = lower_inline_module_from_source(source);
    let semantic = hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");
    QueryExecContext::compile(&module, &type_info)
}

fn shared_forest_fixture_source() -> &'static str {
    r#"
field exact distance shared_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

field exact distance union_leaf_a(p: Vec3) -> F32 {
    translate = vec3(-3.0, 0.0, 0.0) {
        sphere(radius = 0.5)
    }
}

field exact distance union_leaf_b(p: Vec3) -> F32 {
    translate = vec3(-1.0, 0.0, 0.0) {
        sphere(radius = 0.5)
    }
}

field exact distance union_leaf_c(p: Vec3) -> F32 {
    translate = vec3(1.0, 0.0, 0.0) {
        sphere(radius = 0.5)
    }
}

field exact distance union_leaf_d(p: Vec3) -> F32 {
    translate = vec3(3.0, 0.0, 0.0) {
        sphere(radius = 0.5)
    }
}

material shared_material(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.2, 0.3, 0.4),
        roughness=0.25,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape shared_shape {
    field = shared_field
    material = shared_material
}

shape union_leaf_a_shape {
    field = union_leaf_a
    material = shared_material
}

shape union_leaf_b_shape {
    field = union_leaf_b
    material = shared_material
}

shape union_leaf_c_shape {
    field = union_leaf_c
    material = shared_material
}

shape union_leaf_d_shape {
    field = union_leaf_d
    material = shared_material
}

shape large_union_shape {
    union {
        provenance_policy = nearest
        use union_leaf_a_shape
        use union_leaf_b_shape
        use union_leaf_c_shape
        use union_leaf_d_shape
    }
}

region shared_region() {
    place main = shared_shape
}

domain shared_domain(world: RegionCapture) {
    geometry_detail = 1
    material = true
    radiance = false
    media = false
    max_distance = 6.0
    min_step = 0.05
    hit_epsilon = 0.001
    max_steps = 96
}
"#
}

fn mixed_bounded_unbounded_world_fixture_source() -> &'static str {
    r#"
field exact distance distractor_field(p: Vec3) -> F32 {
    translate = vec3(0.0, 3.0, 0.0) {
        sphere(radius = 0.5)
    }
}

field conservative distance repeated_field(p: Vec3) -> F32 {
    repeat_linear = vec3(2.5, 0.0, 0.0) {
        translate = vec3(0.25, 0.0, 0.0) {
            sphere(radius = 0.5)
        }
    }
}

material shared_material(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.2, 0.3, 0.4),
        roughness=0.25,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape distractor_shape {
    field = distractor_field
    material = shared_material
}

shape repeated_shape {
    field = repeated_field
    material = shared_material
}

region mixed_region() {
    place distractor = distractor_shape
    place repeated = repeated_shape
}

domain mixed_domain(world: RegionCapture) {
    geometry_detail = 1
    material = true
    radiance = false
    media = false
    max_distance = 10.0
    min_step = 0.05
    hit_epsilon = 0.001
    max_steps = 96
}
"#
}

fn budget_pressure_fixture_source() -> &'static str {
    r#"
field conservative distance budget_field(p: Vec3) -> F32 {
    box(half = vec3(24.0, 24.0, 24.0))
}

material budget_material(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.2, 0.3, 0.4),
        roughness=0.25,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape budget_shape {
    field = budget_field
    material = budget_material
}
"#
}

#[test]
fn acceleration_forest_builder_is_deterministic() {
    let mut builder = world_forest_builder(shared_world_contract());
    builder
        .push_node({
            let mut node = AccelerationNode::new(
                "leaf_b",
                20,
                AccelerationNodeKind::LeafCandidate,
                AccelerationCandidateClass::SpatialRay,
            )
            .with_child_span(AccelerationChildSpan::new(4, 2))
            .with_leaf_payload(AccelerationLeafPayload::new(
                "leaf.semantic.b",
                Some("feature.b"),
                Some("instance.b"),
                Some("repeat.b"),
            ));
            node.child_ids = vec![SmolStr::new("root_b")];
            node
        })
        .push_node({
            let mut node = AccelerationNode::new(
                "root_a",
                10,
                AccelerationNodeKind::ForestRoot,
                AccelerationCandidateClass::SpatialRay,
            )
            .with_child_span(AccelerationChildSpan::new(0, 2));
            node.child_ids = vec![SmolStr::new("leaf_a"), SmolStr::new("leaf_b")];
            node
        })
        .push_node({
            let mut node = AccelerationNode::new(
                "leaf_a",
                15,
                AccelerationNodeKind::LeafCandidate,
                AccelerationCandidateClass::SpatialRay,
            )
            .with_leaf_payload(AccelerationLeafPayload::new(
                "leaf.semantic.a",
                None::<SmolStr>,
                Some("instance.a"),
                None::<SmolStr>,
            ));
            node.child_ids = vec![SmolStr::new("root_a")];
            node
        })
        .push_cache(AccelerationCacheDescriptor {
            id: SmolStr::new("cache_b"),
            kind: AccelerationCacheKind::DistanceBrickCache,
            scope: CacheArtifactScope::ObserverLocal,
            observer: Some(AccelerationObserver::Query),
            artifact_scope: SmolStr::new("query_helper"),
            fallback_expectation: FallbackExpectation::ConservativeOnly,
        })
        .push_cache(AccelerationCacheDescriptor {
            id: SmolStr::new("cache_a"),
            kind: AccelerationCacheKind::RayCandidateTable,
            scope: CacheArtifactScope::SharedSnapshot,
            observer: Some(AccelerationObserver::Query),
            artifact_scope: SmolStr::new("query_helper"),
            fallback_expectation: FallbackExpectation::None,
        })
        .push_observer_usage(ObserverUsageSummary {
            observer: AccelerationObserver::Collision,
            contract_id: SmolStr::new("collision_plan"),
            used_caches: vec![SmolStr::new("cache_b"), SmolStr::new("cache_a")],
            candidate_classes: vec![AccelerationCandidateClass::CollisionBroadphase],
            notes: vec![SmolStr::new("usage-b")],
        })
        .push_observer_usage(ObserverUsageSummary {
            observer: AccelerationObserver::Query,
            contract_id: SmolStr::new("query_helper"),
            used_caches: vec![SmolStr::new("cache_a")],
            candidate_classes: vec![AccelerationCandidateClass::SpatialRay],
            notes: vec![SmolStr::new("usage-a")],
        })
        .push_rejection(AccelerationRejectionRecord::new(
            AccelerationRejectionClass::ArtifactUnavailable,
            "root_a",
            "analytic evidence is unavailable",
        ))
        .push_rejection(AccelerationRejectionRecord::new(
            AccelerationRejectionClass::OpaqueBoundary,
            "root_b",
            "opaque support blocks coarse pruning",
        ));
    let left = builder.finish();

    let mut right_builder = world_forest_builder(shared_world_contract());
    right_builder
        .push_node({
            let mut node = AccelerationNode::new(
                "root_a",
                10,
                AccelerationNodeKind::ForestRoot,
                AccelerationCandidateClass::SpatialRay,
            )
            .with_child_span(AccelerationChildSpan::new(0, 2));
            node.child_ids = vec![SmolStr::new("leaf_a"), SmolStr::new("leaf_b")];
            node
        })
        .push_node({
            let mut node = AccelerationNode::new(
                "leaf_a",
                15,
                AccelerationNodeKind::LeafCandidate,
                AccelerationCandidateClass::SpatialRay,
            )
            .with_leaf_payload(AccelerationLeafPayload::new(
                "leaf.semantic.a",
                None::<SmolStr>,
                Some("instance.a"),
                None::<SmolStr>,
            ));
            node.child_ids = vec![SmolStr::new("root_a")];
            node
        })
        .push_node({
            let mut node = AccelerationNode::new(
                "leaf_b",
                20,
                AccelerationNodeKind::LeafCandidate,
                AccelerationCandidateClass::SpatialRay,
            )
            .with_child_span(AccelerationChildSpan::new(4, 2))
            .with_leaf_payload(AccelerationLeafPayload::new(
                "leaf.semantic.b",
                Some("feature.b"),
                Some("instance.b"),
                Some("repeat.b"),
            ));
            node.child_ids = vec![SmolStr::new("root_b")];
            node
        })
        .push_cache(AccelerationCacheDescriptor {
            id: SmolStr::new("cache_a"),
            kind: AccelerationCacheKind::RayCandidateTable,
            scope: CacheArtifactScope::SharedSnapshot,
            observer: Some(AccelerationObserver::Query),
            artifact_scope: SmolStr::new("query_helper"),
            fallback_expectation: FallbackExpectation::None,
        })
        .push_cache(AccelerationCacheDescriptor {
            id: SmolStr::new("cache_b"),
            kind: AccelerationCacheKind::DistanceBrickCache,
            scope: CacheArtifactScope::ObserverLocal,
            observer: Some(AccelerationObserver::Query),
            artifact_scope: SmolStr::new("query_helper"),
            fallback_expectation: FallbackExpectation::ConservativeOnly,
        })
        .push_observer_usage(ObserverUsageSummary {
            observer: AccelerationObserver::Query,
            contract_id: SmolStr::new("query_helper"),
            used_caches: vec![SmolStr::new("cache_a")],
            candidate_classes: vec![AccelerationCandidateClass::SpatialRay],
            notes: vec![SmolStr::new("usage-a")],
        })
        .push_observer_usage(ObserverUsageSummary {
            observer: AccelerationObserver::Collision,
            contract_id: SmolStr::new("collision_plan"),
            used_caches: vec![SmolStr::new("cache_b"), SmolStr::new("cache_a")],
            candidate_classes: vec![AccelerationCandidateClass::CollisionBroadphase],
            notes: vec![SmolStr::new("usage-b")],
        })
        .push_rejection(AccelerationRejectionRecord::new(
            AccelerationRejectionClass::OpaqueBoundary,
            "root_b",
            "opaque support blocks coarse pruning",
        ))
        .push_rejection(AccelerationRejectionRecord::new(
            AccelerationRejectionClass::ArtifactUnavailable,
            "root_a",
            "analytic evidence is unavailable",
        ));
    let right = right_builder.finish();

    assert_eq!(left, right);
    assert_eq!(
        left.root_nodes(),
        &[SmolStr::new("root_b"), SmolStr::new("root_a")]
    );
    assert_eq!(left.nodes[0].id, "root_a");
    assert_eq!(left.nodes[1].id, "leaf_a");
    assert_eq!(left.nodes[2].id, "leaf_b");
    assert_eq!(left.rejection_reasons().len(), 2);
    assert!(
        left.rejection_reasons()
            .windows(2)
            .all(|pair| { pair[0].class <= pair[1].class })
    );
}

#[test]
fn acceleration_forest_report_renders_rejection_diagnostics() {
    let mut builder = union_subtree_forest_builder(union_subtree_contract());
    builder
        .push_node({
            let mut node = AccelerationNode::new(
                "union_root",
                1,
                AccelerationNodeKind::ForestRoot,
                AccelerationCandidateClass::CollisionRefinement,
            )
            .with_child_span(AccelerationChildSpan::new(0, 1));
            node.child_ids = vec![SmolStr::new("union_leaf")];
            node
        })
        .push_node(
            AccelerationNode::new(
                "union_leaf",
                2,
                AccelerationNodeKind::LeafCandidate,
                AccelerationCandidateClass::CollisionRefinement,
            )
            .with_leaf_payload(AccelerationLeafPayload::new(
                "union.semantic",
                Some("union.feature"),
                None::<SmolStr>,
                None::<SmolStr>,
            )),
        )
        .push_rejection(AccelerationRejectionRecord::new(
            AccelerationRejectionClass::UnsupportedRepeatForm,
            "union_leaf",
            "repeat form is not safe for shared subtree reuse",
        ));
    let forest = builder.finish();
    let report = AccelerationReport::new(
        AccelerationObserver::Collision,
        vec![forest],
        vec![SmolStr::new("z"), SmolStr::new("a")],
    );
    let dump = report.debug_dump();
    assert_eq!(dump, format!("{}", report));
    assert!(dump.contains("forest id=union_subtree_forest"));
    assert!(dump.contains("child_span=0..1"));
    assert!(dump.contains("leaf semantic_id=union.semantic"));
    assert!(dump.contains("rejection class=unsupported_repeat_form subject=union_leaf"));
    assert!(dump.contains("note a"));
    assert!(dump.contains("note z"));
}

#[test]
fn shared_acceleration_forests_are_built_once_and_are_deterministic() {
    let left = typed_query_context(shared_forest_fixture_source());
    let right = typed_query_context(shared_forest_fixture_source());

    assert_eq!(left.shared_acceleration, right.shared_acceleration);

    let world = left
        .world_acceleration_forest(&SmolStr::new("shared_region"), 0)
        .expect("coarse world forest");
    let world_fine = left
        .world_acceleration_forest(&SmolStr::new("shared_region"), 1)
        .expect("fine world forest");
    let union = left
        .union_acceleration_forest(&SmolStr::new("large_union_shape"))
        .expect("shared union forest");

    assert_eq!(
        world.contract.id,
        "shared_acceleration_forest::shared_region::coarse"
    );
    assert_eq!(
        world_fine.contract.id,
        "shared_acceleration_forest::shared_region::fine"
    );
    assert_eq!(
        union.contract.id,
        "shared_union_subtree_forest::large_union_shape"
    );
    assert_eq!(
        world.root_nodes(),
        &[SmolStr::new(
            "shared_acceleration_forest::shared_region::coarse::root"
        )]
    );
    assert_eq!(
        union.root_nodes(),
        &[SmolStr::new(
            "shared_union_subtree_forest::large_union_shape::root"
        )]
    );
    assert!(world.nodes.iter().any(|node| {
        node.leaf_payload
            .as_ref()
            .is_some_and(|payload| payload.semantic_id == "shared_shape")
    }));
    assert!(union.nodes.iter().any(|node| {
        node.leaf_payload.as_ref().is_some_and(|payload| {
            payload.semantic_id == "large_union_shape" && payload.feature_id.as_deref() == Some("0")
        })
    }));
    assert!(union.nodes.iter().any(|node| node.id.contains("cluster")));
    assert!(left.shared_acceleration.all_forests().len() >= 3);
}

#[test]
fn mixed_bounded_and_unbounded_world_candidates_do_not_publish_partial_cluster_bounds() {
    let ctx = typed_query_context(mixed_bounded_unbounded_world_fixture_source());
    let forest = ctx
        .world_acceleration_forest(&SmolStr::new("mixed_region"), 1)
        .expect("mixed world forest");
    let root = forest
        .nodes
        .iter()
        .find(|node| node.id == "shared_acceleration_forest::mixed_region::fine::root")
        .expect("forest root");
    assert!(root.bounds.is_empty());
    let cluster = forest
        .nodes
        .iter()
        .find(|node| {
            node.id
                .starts_with("shared_acceleration_forest::mixed_region::fine::root::cluster:")
        })
        .expect("mixed cluster");
    assert!(cluster.bounds.is_empty());
}

#[test]
fn shared_cache_catalog_is_deterministic_and_support_bounded() {
    let left = typed_query_context(shared_forest_fixture_source());
    let right = typed_query_context(shared_forest_fixture_source());

    assert_eq!(
        left.shared_acceleration.cache_catalog,
        right.shared_acceleration.cache_catalog
    );

    let shape_support = left
        .shape_cache_support(&SmolStr::new("shared_shape"))
        .expect("shared shape support cache");
    let shape_distance = left
        .shape_cache_distance(&SmolStr::new("shared_shape"))
        .expect("shared shape distance cache");
    assert_eq!(shape_support.schema.version, 1);
    assert_eq!(shape_distance.schema.version, 1);
    assert_eq!(shape_support.schema.semantic_root, "shared_shape");
    assert_eq!(shape_distance.schema.semantic_root, "shared_shape");
    assert_eq!(
        shape_support.report.candidate_bricks,
        shape_distance.report.candidate_bricks
    );
    assert_eq!(
        shape_support.report.rejection_reasons,
        shape_distance.report.rejection_reasons
    );

    let world_support = left
        .world_cache_support(&SmolStr::new("shared_region"), 1)
        .expect("shared world support cache");
    let world_distance = left
        .world_cache_distance(&SmolStr::new("shared_region"), 1)
        .expect("shared world distance cache");
    assert_eq!(world_support.schema.version, 1);
    assert_eq!(world_distance.schema.version, 1);
    assert_eq!(
        world_support.report.candidate_bricks,
        world_distance.report.candidate_bricks
    );
    assert_eq!(
        world_support.report.rejection_reasons,
        world_distance.report.rejection_reasons
    );
}

#[test]
fn cache_catalog_reports_unsupported_support_and_budget_pressure() {
    let ctx = typed_query_context(mixed_bounded_unbounded_world_fixture_source());

    let repeated_shape_cache = ctx
        .shape_cache_support(&SmolStr::new("repeated_shape"))
        .expect("repeated shape cache");
    assert!(!repeated_shape_cache.is_ready());
    assert!(
        repeated_shape_cache
            .report
            .rejection_reasons
            .iter()
            .any(|reason| *reason == CacheDisableReason::UnboundedSupport)
    );

    let world_support = ctx
        .world_cache_support(&SmolStr::new("mixed_region"), 1)
        .expect("mixed region world support cache");
    assert!(world_support.is_ready());
    assert!(
        world_support
            .report
            .rejection_reasons
            .iter()
            .any(|reason| *reason == CacheDisableReason::UnboundedSupport)
    );

    let budget_ctx = typed_query_context(budget_pressure_fixture_source());
    let budget_shape_cache = budget_ctx
        .shape_cache_support(&SmolStr::new("budget_shape"))
        .expect("budget shape cache");
    assert!(!budget_shape_cache.is_ready());
    assert!(
        budget_shape_cache
            .report
            .rejection_reasons
            .iter()
            .any(|reason| *reason == CacheDisableReason::MemoryBudgetExceeded)
    );
    assert_eq!(budget_shape_cache.report.memory_bytes, 0);
}
