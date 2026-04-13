use smol_str::SmolStr;
use wrela::artifact_contract::{
    ArtifactSnapshotRelation, ArtifactUseKind, ArtifactUseSource, SemanticArtifactKind,
};
use wrela::collision_plan::{CollisionPlan, CollisionQueryKind};
use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::parser::ast;
use wrela::parser::ast::AstNode;
use wrela::parser::parse;
use wrela::presentation_plan::PresentationPlan;
use wrela::query_plan::DispatchBackend;
use wrela::query_program_spine::{
    ObserverKind, QueryProgramSpine, SpineAnalysisStatus, SpineDependencyEdge, SpineEdgeKind,
    SpineLossyReason, SpineNodeFamily, project_collision_plan, project_presentation_plan,
    shared_spine_report,
};

fn lower_inline_module(source: &str) -> hir::Module {
    let node = parse(source);
    let root = ast::Root::cast(node).expect("root");
    hir_lower::lower(root)
}

fn presentation_function<'a>(module: &'a hir::Module, name: &str) -> &'a hir::Function {
    module
        .functions
        .iter()
        .find(|(_, func)| func.name == name)
        .map(|(_, func)| func)
        .unwrap_or_else(|| panic!("missing presentation function '{name}'"))
}

fn temporal_view_source() -> &'static str {
    r#"
view temporal_view(world: RegionCapture, camera: Camera) {
    domain = primary_domain(world = world)
    viewport = viewport(width = 4, height = 3)
    quality = realtime_quality(target_fps = 60)
    lighting = key_light(
        light = Light(
            position = camera.position,
            direction = normalize(vec3(0.0, -1.0, 0.0)),
            intensity = vec3(1.0, 1.0, 1.0),
            range = 8.0
        ),
        fill_direction = normalize(vec3(-0.25, 0.6, 0.4)),
        fill_strength = 0.35,
        ambient_color = vec3(0.08, 0.11, 0.14)
    )
    outputs = frame_outputs(color = true, depth = true, normal = true, motion = true)
    history = temporal_history(color = true)
}
"#
}

fn composite_view_source() -> &'static str {
    r#"
view composite_view(world: RegionCapture, camera: Camera) {
    domain = primary_domain(world = world)
    viewport = viewport(width = 4, height = 3)
    quality = realtime_quality(target_fps = 60)
    lighting = key_light(
        light = Light(
            position = camera.position,
            direction = normalize(vec3(0.0, -1.0, 0.0)),
            intensity = vec3(1.0, 1.0, 1.0),
            range = 8.0
        ),
        fill_direction = normalize(vec3(-0.25, 0.6, 0.4)),
        fill_strength = 0.35,
        ambient_color = vec3(0.08, 0.11, 0.14)
    )
    outputs = frame_outputs(color = true, depth = true, normal = true, motion = false)
    history = temporal_history(color = false)
}
"#
}

fn temporal_presentation_plan() -> PresentationPlan {
    let module = lower_inline_module(temporal_view_source());
    let view = presentation_function(&module, "temporal_view");
    PresentationPlan::from_view_function(view, DispatchBackend::Auto).expect("presentation plan")
}

fn composite_presentation_plan() -> PresentationPlan {
    let module = lower_inline_module(composite_view_source());
    let view = presentation_function(&module, "composite_view");
    PresentationPlan::from_view_function(view, DispatchBackend::Auto).expect("presentation plan")
}

fn rename_spine_node_id(spine: &mut QueryProgramSpine, old: &str, new: &str) {
    let new_id = SmolStr::new(new);
    for input in &mut spine.inputs {
        if input.node_id == old {
            input.node_id = new_id.clone();
        }
    }
    for output in &mut spine.outputs {
        if output.node_id == old {
            output.node_id = new_id.clone();
        }
    }
    for node in &mut spine.nodes {
        if node.id == old {
            node.id = new_id.clone();
        }
    }
    for edge in &mut spine.dependencies {
        if edge.from == old {
            edge.from = new_id.clone();
        }
        if edge.to == old {
            edge.to = new_id.clone();
        }
    }
}

#[test]
fn presentation_projection_is_deterministic_and_exposes_shared_artifact_reuse() {
    let plan = temporal_presentation_plan();
    let projection = project_presentation_plan(&plan);

    assert_eq!(projection, project_presentation_plan(&plan));
    assert_eq!(projection.observer_kind, ObserverKind::Presentation);
    assert_eq!(projection.source_plan, "temporal_view");
    assert_eq!(
        projection
            .spine
            .inputs
            .iter()
            .map(|binding| binding.binding.as_str())
            .collect::<Vec<_>>(),
        vec!["world", "camera"]
    );
    assert_eq!(
        projection
            .spine
            .primitive_nodes()
            .map(|node| node.label.as_str())
            .collect::<Vec<_>>(),
        vec![
            "generate_screen_samples",
            "primary_visibility",
            "surface_resolve",
            "participants_resolve",
            "shade_primary",
            "motion_resolve",
            "temporal_resolve",
        ]
    );
    assert!(projection.spine.semantic_artifacts.iter().any(|artifact| {
        artifact.id == "artifact.history_color"
            && artifact.kind == SemanticArtifactKind::PresentationHistory
            && artifact.validity.is_explicit()
    }));
    assert!(projection.spine.artifact_uses.iter().any(|use_record| {
        use_record.artifact_id == "artifact.history_color"
            && use_record.kind == ArtifactUseKind::Load
            && use_record.source == ArtifactUseSource::ArtifactStore
    }));
    assert!(projection.spine.artifact_uses.iter().any(|use_record| {
        use_record.artifact_id == "artifact.history_color"
            && use_record.kind == ArtifactUseKind::Preserve
    }));
    assert_eq!(
        projection
            .spine
            .dependencies
            .iter()
            .filter(|edge| edge.kind == SpineEdgeKind::ConsumesInput)
            .map(|edge| {
                (
                    edge.from.as_str(),
                    edge.to.as_str(),
                    edge.subject.as_ref().map(|subject| subject.as_str()),
                )
            })
            .collect::<Vec<_>>(),
        vec![
            ("input:world", "invoke:primary_visibility", Some("world"),),
            ("input:camera", "invoke:primary_visibility", Some("camera"),),
            ("input:world", "invoke:surface_resolve", Some("world")),
            ("input:world", "invoke:participants_resolve", Some("world"),),
        ]
    );
    assert!(
        projection
            .spine
            .outputs
            .iter()
            .any(|binding| binding.binding == "color")
    );
    assert_eq!(
        projection
            .lossy_boundaries
            .iter()
            .map(|boundary| {
                (
                    boundary.node_id.as_str(),
                    boundary.reason,
                    boundary
                        .dropped_fields
                        .iter()
                        .map(|field| field.as_str())
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>(),
        vec![
            (
                "projection:presentation",
                SpineLossyReason::ObserverSpecificSemantics,
                vec![
                    "view_contract",
                    "frame_contract",
                    "lighting_contract",
                    "binding_execution",
                ],
            ),
            (
                "policy:presentation_backend_dispatch",
                SpineLossyReason::BackendKernel,
                vec!["backend_kernels", "host_export_wiring"],
            ),
            (
                "observability:presentation",
                SpineLossyReason::RuntimeTrace,
                vec!["frame_cost_history", "attachment_debug_payloads"],
            ),
            (
                "invoke:temporal_resolve",
                SpineLossyReason::TemporalDetail,
                vec![
                    "history_weight_numerator",
                    "history_weight_denominator",
                    "neighborhood_clamp",
                ],
            ),
        ]
    );
    assert!(
        projection
            .spine
            .dependencies
            .iter()
            .any(|edge| edge.to == "output:history_color" || edge.to == "output:color")
    );
}

#[test]
fn collision_projection_is_deterministic_and_exposes_policy_and_store_loads() {
    let plan = CollisionPlan::for_query_with_backend(
        CollisionQueryKind::SphereSweepTransition,
        DispatchBackend::Auto,
    );
    let projection = project_collision_plan(&plan);

    assert_eq!(projection, project_collision_plan(&plan));
    assert_eq!(projection.observer_kind, ObserverKind::Collision);
    assert_eq!(projection.source_plan, "collision.sphere_sweep.transition");
    assert_eq!(
        projection
            .spine
            .inputs
            .iter()
            .map(|binding| binding.binding.as_str())
            .collect::<Vec<_>>(),
        vec!["world", "domain", "transition", "sweep"]
    );
    assert_eq!(
        projection
            .spine
            .primitive_nodes()
            .map(|node| node.label.as_str())
            .collect::<Vec<_>>(),
        vec![
            "gather_candidates",
            "build_broadphase_candidates",
            "sweep_sphere_first_contact",
            "materialize_output",
        ]
    );
    assert!(projection.spine.nodes.iter().any(|node| {
        node.family == SpineNodeFamily::PolicyRequirement
            && node
                .notes
                .iter()
                .any(|note| note == "required_guarantee=conservative_no_false_miss")
    }));
    assert!(projection.spine.artifact_uses.iter().any(|use_record| {
        use_record.artifact_id == "artifact.witness_cache.sphere_sweep"
            && use_record.kind == ArtifactUseKind::Load
            && use_record.source == ArtifactUseSource::ArtifactStore
    }));
    assert!(projection.spine.artifact_uses.iter().any(|use_record| {
        use_record.artifact_id == "artifact.continuation_seed.sphere_sweep"
            && use_record.kind == ArtifactUseKind::Load
            && use_record.source == ArtifactUseSource::ArtifactStore
    }));
    assert_eq!(
        projection
            .spine
            .dependencies
            .iter()
            .filter(|edge| edge.kind == SpineEdgeKind::ConsumesInput)
            .map(|edge| {
                (
                    edge.from.as_str(),
                    edge.to.as_str(),
                    edge.subject.as_ref().map(|subject| subject.as_str()),
                )
            })
            .collect::<Vec<_>>(),
        vec![
            ("input:world", "invoke:candidate_gather", Some("world")),
            ("input:domain", "invoke:candidate_gather", Some("domain")),
            ("input:sweep", "invoke:candidate_gather", Some("sweep")),
            ("input:world", "invoke:sphere_sweep", Some("world")),
            ("input:domain", "invoke:sphere_sweep", Some("domain")),
            (
                "input:transition",
                "invoke:sphere_sweep",
                Some("transition"),
            ),
            ("input:sweep", "invoke:sphere_sweep", Some("sweep")),
        ]
    );
    assert_eq!(
        projection
            .lossy_boundaries
            .iter()
            .map(|boundary| {
                (
                    boundary.node_id.as_str(),
                    boundary.reason,
                    boundary
                        .dropped_fields
                        .iter()
                        .map(|field| field.as_str())
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>(),
        vec![
            (
                "projection:collision",
                SpineLossyReason::ObserverSpecificSemantics,
                vec!["collision_contract_descriptor", "witness_payload_semantics"],
            ),
            (
                "policy:collision",
                SpineLossyReason::PolicyAuthority,
                vec!["full_authority_contract", "backend_legality_checks"],
            ),
            (
                "observability:collision",
                SpineLossyReason::RuntimeTrace,
                vec!["artifact_store", "reuse_metrics", "reuse_decisions"],
            ),
        ]
    );
    let sweep_node = projection
        .spine
        .nodes
        .iter()
        .find(|node| node.id == "invoke:sphere_sweep")
        .expect("sphere_sweep node");
    assert_eq!(
        sweep_node
            .artifact_ids
            .iter()
            .map(|artifact_id| artifact_id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "artifact.support_summary.sphere_sweep",
            "artifact.broadphase_candidates.sphere_sweep",
        ]
    );
    assert!(projection.spine.dependencies.iter().any(|edge| {
        edge.from == "invoke:sphere_sweep"
            && edge.to == "invoke:materialize_output"
            && edge.kind == SpineEdgeKind::ConsumesValue
            && edge
                .subject
                .as_ref()
                .is_some_and(|subject| subject == "sweep_contact")
    }));
    assert!(
        projection
            .spine
            .dependencies
            .iter()
            .any(|edge| edge.from == "invoke:materialize_output"
                && edge.to == "output:sweep_contact"
                && edge.kind == SpineEdgeKind::FeedsOutput)
    );
}

#[test]
fn presentation_and_collision_share_the_same_broad_spine_vocabulary() {
    let presentation = project_presentation_plan(&composite_presentation_plan());
    let collision = project_collision_plan(&CollisionPlan::for_query(
        CollisionQueryKind::PointOccupancyWorld,
    ));

    for family in [
        SpineNodeFamily::InputBinding,
        SpineNodeFamily::PrimitiveInvocation,
        SpineNodeFamily::ArtifactStore,
        SpineNodeFamily::PolicyRequirement,
        SpineNodeFamily::OutputBinding,
        SpineNodeFamily::ObservabilitySummary,
    ] {
        assert!(
            presentation
                .spine
                .nodes
                .iter()
                .any(|node| node.family == family),
            "presentation missing {:?}",
            family
        );
        assert!(
            collision
                .spine
                .nodes
                .iter()
                .any(|node| node.family == family),
            "collision missing {:?}",
            family
        );
    }
    assert!(
        !presentation
            .lossy_boundaries
            .iter()
            .any(|boundary| boundary.reason == SpineLossyReason::TemporalDetail)
    );
    assert!(presentation.spine.observability.runtime_trace_local_only);
    assert!(collision.spine.observability.runtime_trace_local_only);
    assert_ne!(
        presentation.spine.observer_kind,
        collision.spine.observer_kind
    );
}

#[test]
fn shared_spine_report_proves_dependency_and_lifetime_analysis_for_presentation() {
    let projection = project_presentation_plan(&temporal_presentation_plan());
    let report = shared_spine_report(&projection);

    assert_eq!(report.status, SpineAnalysisStatus::Valid);
    assert_eq!(report.dependency.status, SpineAnalysisStatus::Valid);
    assert!(report.dependency.missing_node_edges.is_empty());
    assert!(report.dependency.cycles.iter().any(|cycle| {
        cycle
            .nodes
            .iter()
            .any(|node_id| node_id == "load:temporal_resolve:artifact.history_color")
            && cycle
                .nodes
                .iter()
                .any(|node_id| node_id == "artifact:artifact.history_color")
    }));
    assert!(
        report
            .dependency
            .roots
            .iter()
            .any(|node_id| node_id == "policy:presentation_backend_dispatch")
    );
    assert!(
        report
            .dependency
            .leaves
            .iter()
            .any(|node_id| node_id == "output:color")
    );
    assert_eq!(report.artifact_lifetime.status, SpineAnalysisStatus::Valid);
    assert!(
        report
            .artifact_lifetime
            .contract_checks
            .iter()
            .any(|check| {
                check.artifact_id == "artifact.history_color"
                    && check.status == SpineAnalysisStatus::Valid
                    && check
                        .load_node_ids
                        .iter()
                        .any(|node_id| node_id == "load:temporal_resolve:artifact.history_color")
            })
    );
    assert!(report.backend.dispatch_observable);
    assert!(report.observability.local_only.runtime_trace_local_only);
    assert!(report.observability.local_only.observer_metrics_local_only);
}

#[test]
fn shared_spine_policy_summary_flags_illegal_collision_backends() {
    let exact_projection = project_collision_plan(&CollisionPlan::for_query_with_backend(
        CollisionQueryKind::PointOccupancyWorld,
        DispatchBackend::Wgsl,
    ));
    let exact_report = shared_spine_report(&exact_projection);
    let unsupported_projection = project_collision_plan(&CollisionPlan::for_query_with_backend(
        CollisionQueryKind::SphereSweepTransition,
        DispatchBackend::Wgsl,
    ));
    let unsupported_report = shared_spine_report(&unsupported_projection);

    assert_eq!(exact_report.observer_kind, ObserverKind::Collision);
    assert_eq!(exact_report.execution_owner, "CollisionPlan");
    assert_eq!(exact_report.policy.status, SpineAnalysisStatus::Invalid);
    assert!(
        exact_report
            .policy
            .illegal_combinations
            .iter()
            .any(|reason| {
                reason.contains("backend=wgsl")
                    && (reason.contains("method=exact_oracle")
                        || reason.contains("supported_backends=cpu"))
            })
    );
    assert!(exact_report.backend.dispatch_observable);
    assert_eq!(
        unsupported_report.policy.status,
        SpineAnalysisStatus::Invalid
    );
    assert!(
        unsupported_report
            .policy
            .illegal_combinations
            .iter()
            .any(|reason| reason.contains("supported_backends=cpu"))
    );
    assert!(
        exact_report
            .observability
            .lossy_boundaries
            .iter()
            .any(|boundary| boundary.reason == "policy_authority")
    );
}

#[test]
fn shared_spine_lifetime_validation_rejects_missing_store_load_edges() {
    let mut projection = project_presentation_plan(&temporal_presentation_plan());
    projection.spine.dependencies.retain(|edge| {
        !(edge.kind == SpineEdgeKind::LoadsArtifact
            && edge
                .subject
                .as_ref()
                .is_some_and(|subject| subject == "artifact.history_color"))
    });

    let report = shared_spine_report(&projection);

    assert_eq!(
        report.artifact_lifetime.status,
        SpineAnalysisStatus::Invalid
    );
    assert!(report.artifact_lifetime.use_checks.iter().any(|check| {
        check.artifact_id == "artifact.history_color"
            && check.actor == "temporal_resolve"
            && check
                .notes
                .iter()
                .any(|note| note == "missing_store_load_edge")
    }));
}

#[test]
fn shared_spine_dependency_rejects_mixed_temporal_feedback_cycle() {
    let mut projection = project_collision_plan(&CollisionPlan::for_query_with_backend(
        CollisionQueryKind::SphereSweepTransition,
        DispatchBackend::Auto,
    ));
    projection.spine.dependencies.push(SpineDependencyEdge {
        from: SmolStr::new("invoke:materialize_output"),
        to: SmolStr::new("invoke:sphere_sweep"),
        kind: SpineEdgeKind::ConsumesValue,
        subject: Some(SmolStr::new("unexpected_feedback")),
        required_validity: None,
        lossy: false,
    });

    let report = shared_spine_report(&projection);

    assert_eq!(report.dependency.status, SpineAnalysisStatus::Invalid);
    assert!(report.dependency.cycles.iter().any(|cycle| {
        cycle
            .nodes
            .iter()
            .any(|node_id| node_id == "invoke:materialize_output")
            && cycle
                .nodes
                .iter()
                .any(|node_id| node_id == "invoke:sphere_sweep")
    }));
}

#[test]
fn shared_spine_lifetime_validation_uses_spine_structure_not_generated_ids() {
    let mut projection = project_presentation_plan(&temporal_presentation_plan());
    rename_spine_node_id(
        &mut projection.spine,
        "invoke:temporal_resolve",
        "primitive:temporal_resolve",
    );
    rename_spine_node_id(
        &mut projection.spine,
        "artifact:artifact.history_color",
        "cache:history_color",
    );
    rename_spine_node_id(
        &mut projection.spine,
        "load:temporal_resolve:artifact.history_color",
        "artifact_load:history_color",
    );

    let report = shared_spine_report(&projection);

    assert_eq!(report.status, SpineAnalysisStatus::Valid);
    assert_eq!(report.dependency.status, SpineAnalysisStatus::Valid);
    assert_eq!(report.artifact_lifetime.status, SpineAnalysisStatus::Valid);
    assert!(report.artifact_lifetime.use_checks.iter().any(|check| {
        check.artifact_id == "artifact.history_color"
            && check.actor == "temporal_resolve"
            && check.status == SpineAnalysisStatus::Valid
    }));
}

#[test]
fn shared_spine_collision_contract_validation_uses_semantic_metadata_not_labels() {
    let mut projection = project_collision_plan(&CollisionPlan::for_query_with_backend(
        CollisionQueryKind::SphereSweepTransition,
        DispatchBackend::Auto,
    ));
    let artifact_id = SmolStr::new("artifact.witness_cache.sphere_sweep");
    projection
        .spine
        .nodes
        .iter_mut()
        .find(|node| {
            node.family == SpineNodeFamily::ArtifactStore
                && node.artifact_ids.first() == Some(&artifact_id)
        })
        .expect("witness cache node")
        .label = SmolStr::new("renamed_transition_artifact");
    projection
        .spine
        .semantic_artifacts
        .iter_mut()
        .find(|contract| contract.id == artifact_id)
        .expect("witness cache contract")
        .compatibility
        .snapshot = ArtifactSnapshotRelation::ExactSnapshot;

    let report = shared_spine_report(&projection);

    assert_eq!(
        report.artifact_lifetime.status,
        SpineAnalysisStatus::Invalid
    );
    assert!(
        report
            .artifact_lifetime
            .contract_checks
            .iter()
            .any(|check| {
                check.artifact_id == artifact_id
                    && check.notes.iter().any(|note| {
                        note == "collision_transition_artifact_requires_previous_snapshot_scope"
                    })
            })
    );
}
