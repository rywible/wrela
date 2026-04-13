use crate::artifact_contract::{
    ArtifactUse, ArtifactUseKind, ArtifactUseSource, ArtifactValidityRule, SemanticArtifactContract,
};
use crate::collision_contract::{
    collision_authority_scope_name, collision_input_kind_name, collision_output_kind_name,
};
use crate::collision_plan::{CollisionPassKind, CollisionPlan, collision_artifact_kind_name};
use crate::presentation_contract::{
    AttachmentClearPolicy, AttachmentElementSchema, AttachmentLifetime, AttachmentResolutionClass,
    AttachmentResolutionScale, CanonicalProjectionInput, CanonicalViewRaySpace,
    FrameAttachmentKind,
};
use crate::presentation_plan::{PresentationPassKind, PresentationPlan};
use smol_str::SmolStr;
use std::collections::{BTreeMap, BTreeSet};

pub const QUERY_PROGRAM_SPINE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ObserverKind {
    Presentation,
    Collision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObserverProjection {
    pub observer_kind: ObserverKind,
    pub source_plan: SmolStr,
    pub execution_owner: SmolStr,
    pub observer_local_notes: Vec<SmolStr>,
    pub lossy_boundaries: Vec<SpineLossyBoundary>,
    pub spine: QueryProgramSpine,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryProgramSpine {
    pub schema_version: u32,
    pub observer_kind: ObserverKind,
    pub source_plan: SmolStr,
    pub inputs: Vec<SpineInputBinding>,
    pub nodes: Vec<SpineNode>,
    pub dependencies: Vec<SpineDependencyEdge>,
    pub outputs: Vec<SpineOutputBinding>,
    pub semantic_artifacts: Vec<SemanticArtifactContract>,
    pub artifact_uses: Vec<ArtifactUse>,
    pub observability: SpineObservabilitySummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpineInputBinding {
    pub node_id: SmolStr,
    pub binding: SmolStr,
    pub schema: SmolStr,
    pub role: SmolStr,
    pub notes: Vec<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpineOutputBinding {
    pub node_id: SmolStr,
    pub binding: SmolStr,
    pub schema: SmolStr,
    pub role: SmolStr,
    pub notes: Vec<SmolStr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SpineNodeFamily {
    InputBinding,
    PrimitiveInvocation,
    ArtifactLoad,
    ArtifactStore,
    PolicyRequirement,
    OutputBinding,
    ObservabilitySummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpineNode {
    pub id: SmolStr,
    pub family: SpineNodeFamily,
    pub label: SmolStr,
    pub query_contracts: Vec<SmolStr>,
    pub artifact_ids: Vec<SmolStr>,
    pub required_validity: Option<ArtifactValidityRule>,
    pub notes: Vec<SmolStr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SpineEdgeKind {
    ConsumesInput,
    ConsumesValue,
    ConsumesArtifact,
    ProducesArtifact,
    LoadsArtifact,
    StoresArtifact,
    RequiresPolicy,
    FeedsOutput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpineDependencyEdge {
    pub from: SmolStr,
    pub to: SmolStr,
    pub kind: SpineEdgeKind,
    pub subject: Option<SmolStr>,
    pub required_validity: Option<ArtifactValidityRule>,
    pub lossy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SpineLossyReason {
    ObserverSpecificSemantics,
    RuntimeTrace,
    BackendKernel,
    PolicyAuthority,
    TemporalDetail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpineLossyBoundary {
    pub node_id: SmolStr,
    pub reason: SpineLossyReason,
    pub dropped_fields: Vec<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpineObservabilitySummary {
    pub graph_structure: bool,
    pub artifact_lifecycle: bool,
    pub query_dependencies: bool,
    pub backend_dispatch: bool,
    pub output_bindings: bool,
    pub validation_summary: bool,
    pub runtime_trace_local_only: bool,
    pub observer_metrics_local_only: bool,
}

impl QueryProgramSpine {
    pub fn primitive_nodes(&self) -> impl Iterator<Item = &SpineNode> {
        self.nodes
            .iter()
            .filter(|node| node.family == SpineNodeFamily::PrimitiveInvocation)
    }
}

pub fn observer_kind_name(kind: ObserverKind) -> &'static str {
    match kind {
        ObserverKind::Presentation => "presentation",
        ObserverKind::Collision => "collision",
    }
}

pub fn spine_node_family_name(family: SpineNodeFamily) -> &'static str {
    match family {
        SpineNodeFamily::InputBinding => "input_binding",
        SpineNodeFamily::PrimitiveInvocation => "primitive_invocation",
        SpineNodeFamily::ArtifactLoad => "artifact_load",
        SpineNodeFamily::ArtifactStore => "artifact_store",
        SpineNodeFamily::PolicyRequirement => "policy_requirement",
        SpineNodeFamily::OutputBinding => "output_binding",
        SpineNodeFamily::ObservabilitySummary => "observability_summary",
    }
}

pub fn spine_edge_kind_name(kind: SpineEdgeKind) -> &'static str {
    match kind {
        SpineEdgeKind::ConsumesInput => "consumes_input",
        SpineEdgeKind::ConsumesValue => "consumes_value",
        SpineEdgeKind::ConsumesArtifact => "consumes_artifact",
        SpineEdgeKind::ProducesArtifact => "produces_artifact",
        SpineEdgeKind::LoadsArtifact => "loads_artifact",
        SpineEdgeKind::StoresArtifact => "stores_artifact",
        SpineEdgeKind::RequiresPolicy => "requires_policy",
        SpineEdgeKind::FeedsOutput => "feeds_output",
    }
}

pub fn spine_lossy_reason_name(reason: SpineLossyReason) -> &'static str {
    match reason {
        SpineLossyReason::ObserverSpecificSemantics => "observer_specific_semantics",
        SpineLossyReason::RuntimeTrace => "runtime_trace",
        SpineLossyReason::BackendKernel => "backend_kernel",
        SpineLossyReason::PolicyAuthority => "policy_authority",
        SpineLossyReason::TemporalDetail => "temporal_detail",
    }
}

pub fn project_presentation_plan(plan: &PresentationPlan) -> ObserverProjection {
    let semantic_artifacts = plan.semantic_artifact_contracts();
    let artifact_uses = plan.artifact_uses();
    let artifact_contracts = semantic_artifacts
        .iter()
        .cloned()
        .map(|contract| (contract.id.clone(), contract))
        .collect::<BTreeMap<_, _>>();
    let attachment_to_artifact = plan
        .frame_artifacts
        .iter()
        .map(|artifact| (artifact.attachment.clone(), artifact.id.clone()))
        .collect::<BTreeMap<_, _>>();

    let inputs = vec![
        SpineInputBinding {
            node_id: SmolStr::new("input:world"),
            binding: SmolStr::new("world"),
            schema: SmolStr::new("RegionCapture"),
            role: SmolStr::new("captured_region"),
            notes: vec![SmolStr::new(
                "presentation execution remains tied to a captured region",
            )],
        },
        SpineInputBinding {
            node_id: SmolStr::new("input:camera"),
            binding: SmolStr::new("camera"),
            schema: SmolStr::new("Camera"),
            role: SmolStr::new("view_camera"),
            notes: vec![SmolStr::new(format!(
                "canonical_projection_input={}",
                canonical_projection_input_name(plan.view.canonical_projection_input)
            ))],
        },
    ];
    let outputs = plan
        .frame
        .outputs
        .iter()
        .map(|output| SpineOutputBinding {
            node_id: SmolStr::new(format!("output:{}", output.name)),
            binding: output.name.clone(),
            schema: SmolStr::new(attachment_element_schema_name(&output.element_schema)),
            role: SmolStr::new(frame_attachment_kind_name(output.kind)),
            notes: vec![
                SmolStr::new(format!(
                    "lifetime={}",
                    attachment_lifetime_name(output.lifetime)
                )),
                SmolStr::new(format!(
                    "resolution={}",
                    attachment_resolution_name(output.resolution)
                )),
                SmolStr::new(format!(
                    "scale={}",
                    attachment_resolution_scale_name(output.scale)
                )),
                SmolStr::new(format!(
                    "clear_policy={}",
                    attachment_clear_policy_name(output.clear_policy)
                )),
            ],
        })
        .collect::<Vec<_>>();

    let policy_node_id = SmolStr::new("policy:presentation_backend_dispatch");
    let unique_backends = plan
        .bindings
        .iter()
        .map(|binding| dispatch_backend_name(binding.default_backend))
        .collect::<BTreeSet<_>>();
    let mut nodes = inputs
        .iter()
        .map(spine_input_node)
        .collect::<Vec<SpineNode>>();
    nodes.push(SpineNode {
        id: policy_node_id.clone(),
        family: SpineNodeFamily::PolicyRequirement,
        label: SmolStr::new("presentation_backend_dispatch"),
        query_contracts: Vec::new(),
        artifact_ids: Vec::new(),
        required_validity: None,
        notes: vec![
            SmolStr::new(format!(
                "default_backends={}",
                join_display(unique_backends.iter().copied())
            )),
            SmolStr::new(format!("binding_count={}", plan.bindings.len())),
            SmolStr::new(format!(
                "view_ray_space={}",
                view_ray_space_name(plan.view.canonical_view_ray.space)
            )),
        ],
    });

    let primitive_nodes = plan
        .passes
        .iter()
        .map(|pass| {
            let mut artifact_ids = pass
                .consumes
                .iter()
                .filter_map(|attachment| attachment_to_artifact.get(attachment).cloned())
                .collect::<Vec<_>>();
            artifact_ids.extend(
                pass.materializes
                    .iter()
                    .filter_map(|attachment| attachment_to_artifact.get(attachment).cloned()),
            );
            SpineNode {
                id: SmolStr::new(format!("invoke:{}", pass.id)),
                family: SpineNodeFamily::PrimitiveInvocation,
                label: SmolStr::new(presentation_pass_spine_label(&pass.kind)),
                query_contracts: pass
                    .query_dependencies
                    .iter()
                    .map(|contract| SmolStr::new(contract.as_str()))
                    .collect(),
                artifact_ids,
                required_validity: None,
                notes: primitive_presentation_notes(pass),
            }
        })
        .collect::<Vec<_>>();
    nodes.extend(primitive_nodes);

    nodes.extend(plan.frame_artifacts.iter().map(|artifact| {
        let contract = artifact_contracts
            .get(&artifact.id)
            .expect("presentation artifact should have semantic contract");
        SpineNode {
            id: SmolStr::new(format!("artifact:{}", artifact.id)),
            family: SpineNodeFamily::ArtifactStore,
            label: artifact.attachment.clone(),
            query_contracts: Vec::new(),
            artifact_ids: vec![artifact.id.clone()],
            required_validity: Some(contract.validity.clone()),
            notes: vec![
                SmolStr::new(format!("contract_id={}", contract.id)),
                SmolStr::new(format!("producer_pass={}", artifact.producer_pass)),
                SmolStr::new(format!("materialized={}", artifact.materialized)),
            ],
        }
    }));

    nodes.extend(
        artifact_uses
            .iter()
            .filter(|use_record| use_record.source == ArtifactUseSource::ArtifactStore)
            .map(|use_record| SpineNode {
                id: SmolStr::new(format!(
                    "load:{}:{}",
                    use_record.actor, use_record.artifact_id
                )),
                family: SpineNodeFamily::ArtifactLoad,
                label: use_record.artifact_id.clone(),
                query_contracts: Vec::new(),
                artifact_ids: vec![use_record.artifact_id.clone()],
                required_validity: use_record.required_validity.clone(),
                notes: vec![SmolStr::new(format!("actor={}", use_record.actor))],
            }),
    );

    nodes.extend(outputs.iter().map(spine_output_node));

    let observability = SpineObservabilitySummary {
        graph_structure: plan.observability.pass_graph,
        artifact_lifecycle: !artifact_uses.is_empty(),
        query_dependencies: plan.observability.query_dependencies,
        backend_dispatch: plan.observability.backend_dispatch,
        output_bindings: !outputs.is_empty(),
        validation_summary: true,
        runtime_trace_local_only: true,
        observer_metrics_local_only: true,
    };
    nodes.push(SpineNode {
        id: SmolStr::new("observability:presentation"),
        family: SpineNodeFamily::ObservabilitySummary,
        label: SmolStr::new("presentation_observability"),
        query_contracts: Vec::new(),
        artifact_ids: Vec::new(),
        required_validity: None,
        notes: observability_notes(&observability),
    });

    let mut dependencies = Vec::new();
    for pass in &plan.passes {
        let pass_node_id = SmolStr::new(format!("invoke:{}", pass.id));
        for input in presentation_input_dependencies(&pass.kind) {
            dependencies.push(SpineDependencyEdge {
                from: SmolStr::new(format!("input:{input}")),
                to: pass_node_id.clone(),
                kind: SpineEdgeKind::ConsumesInput,
                subject: Some(SmolStr::new(input)),
                required_validity: None,
                lossy: false,
            });
        }
        dependencies.push(SpineDependencyEdge {
            from: policy_node_id.clone(),
            to: pass_node_id.clone(),
            kind: SpineEdgeKind::RequiresPolicy,
            subject: None,
            required_validity: None,
            lossy: false,
        });
        for use_record in artifact_uses
            .iter()
            .filter(|use_record| use_record.actor == pass.id)
        {
            let artifact_node_id = SmolStr::new(format!("artifact:{}", use_record.artifact_id));
            match (use_record.kind, use_record.source) {
                (ArtifactUseKind::Load, ArtifactUseSource::Plan) => {
                    dependencies.push(SpineDependencyEdge {
                        from: artifact_node_id,
                        to: pass_node_id.clone(),
                        kind: SpineEdgeKind::ConsumesArtifact,
                        subject: Some(use_record.artifact_id.clone()),
                        required_validity: None,
                        lossy: false,
                    });
                }
                (ArtifactUseKind::Load, ArtifactUseSource::ArtifactStore) => {
                    let load_node_id = SmolStr::new(format!(
                        "load:{}:{}",
                        use_record.actor, use_record.artifact_id
                    ));
                    dependencies.push(SpineDependencyEdge {
                        from: artifact_node_id.clone(),
                        to: load_node_id.clone(),
                        kind: SpineEdgeKind::LoadsArtifact,
                        subject: Some(use_record.artifact_id.clone()),
                        required_validity: use_record.required_validity.clone(),
                        lossy: false,
                    });
                    dependencies.push(SpineDependencyEdge {
                        from: load_node_id,
                        to: pass_node_id.clone(),
                        kind: SpineEdgeKind::ConsumesArtifact,
                        subject: Some(use_record.artifact_id.clone()),
                        required_validity: use_record.required_validity.clone(),
                        lossy: false,
                    });
                }
                (ArtifactUseKind::Produce, ArtifactUseSource::Plan) => {
                    dependencies.push(SpineDependencyEdge {
                        from: pass_node_id.clone(),
                        to: artifact_node_id,
                        kind: SpineEdgeKind::ProducesArtifact,
                        subject: Some(use_record.artifact_id.clone()),
                        required_validity: None,
                        lossy: false,
                    });
                }
                (ArtifactUseKind::Preserve, ArtifactUseSource::Plan) => {
                    dependencies.push(SpineDependencyEdge {
                        from: pass_node_id.clone(),
                        to: artifact_node_id,
                        kind: SpineEdgeKind::StoresArtifact,
                        subject: Some(use_record.artifact_id.clone()),
                        required_validity: use_record.required_validity.clone(),
                        lossy: false,
                    });
                }
                (ArtifactUseKind::Produce, ArtifactUseSource::ArtifactStore)
                | (ArtifactUseKind::Preserve, ArtifactUseSource::ArtifactStore) => {}
            }
        }
    }
    for output in &outputs {
        if let Some(artifact_id) = attachment_to_artifact.get(&output.binding) {
            dependencies.push(SpineDependencyEdge {
                from: SmolStr::new(format!("artifact:{}", artifact_id)),
                to: output.node_id.clone(),
                kind: SpineEdgeKind::FeedsOutput,
                subject: Some(output.binding.clone()),
                required_validity: None,
                lossy: false,
            });
        }
    }

    let mut lossy_boundaries = vec![
        SpineLossyBoundary {
            node_id: SmolStr::new("projection:presentation"),
            reason: SpineLossyReason::ObserverSpecificSemantics,
            dropped_fields: vec![
                SmolStr::new("view_contract"),
                SmolStr::new("frame_contract"),
                SmolStr::new("lighting_contract"),
                SmolStr::new("binding_execution"),
            ],
        },
        SpineLossyBoundary {
            node_id: SmolStr::new("policy:presentation_backend_dispatch"),
            reason: SpineLossyReason::BackendKernel,
            dropped_fields: vec![
                SmolStr::new("backend_kernels"),
                SmolStr::new("host_export_wiring"),
            ],
        },
        SpineLossyBoundary {
            node_id: SmolStr::new("observability:presentation"),
            reason: SpineLossyReason::RuntimeTrace,
            dropped_fields: vec![
                SmolStr::new("frame_cost_history"),
                SmolStr::new("attachment_debug_payloads"),
            ],
        },
    ];
    if plan
        .passes
        .iter()
        .any(|pass| matches!(pass.kind, PresentationPassKind::TemporalResolve { .. }))
    {
        lossy_boundaries.push(SpineLossyBoundary {
            node_id: SmolStr::new("invoke:temporal_resolve"),
            reason: SpineLossyReason::TemporalDetail,
            dropped_fields: vec![
                SmolStr::new("history_weight_numerator"),
                SmolStr::new("history_weight_denominator"),
                SmolStr::new("neighborhood_clamp"),
            ],
        });
    }

    ObserverProjection {
        observer_kind: ObserverKind::Presentation,
        source_plan: plan.name.clone(),
        execution_owner: SmolStr::new("PresentationPlan"),
        observer_local_notes: vec![
            SmolStr::new("presentation plans remain the execution owners"),
            SmolStr::new("screen-lattice, view-ray, and shading semantics remain observer-local"),
        ],
        lossy_boundaries: lossy_boundaries.clone(),
        spine: QueryProgramSpine {
            schema_version: QUERY_PROGRAM_SPINE_SCHEMA_VERSION,
            observer_kind: ObserverKind::Presentation,
            source_plan: plan.name.clone(),
            inputs,
            nodes,
            dependencies,
            outputs,
            semantic_artifacts,
            artifact_uses,
            observability,
        },
    }
}

pub fn project_collision_plan(plan: &CollisionPlan) -> ObserverProjection {
    let semantic_artifacts = plan.semantic_artifact_contracts();
    let artifact_uses = plan.artifact_uses();
    let outputs = plan
        .outputs
        .iter()
        .map(|output| SpineOutputBinding {
            node_id: SmolStr::new(format!("output:{}", output.name)),
            binding: output.name.clone(),
            schema: output.record.clone(),
            role: SmolStr::new(collision_output_kind_name(output.kind)),
            notes: output
                .witness_schema
                .map(|schema| vec![SmolStr::new(format!("witness_schema={}", schema.name))])
                .unwrap_or_default(),
        })
        .collect::<Vec<_>>();
    let inputs = plan
        .inputs
        .iter()
        .map(|input| SpineInputBinding {
            node_id: SmolStr::new(format!("input:{}", input.name)),
            binding: input.name.clone(),
            schema: input.record.clone(),
            role: SmolStr::new(collision_input_kind_name(input.kind)),
            notes: Vec::new(),
        })
        .collect::<Vec<_>>();
    let mut nodes = inputs
        .iter()
        .map(spine_input_node)
        .collect::<Vec<SpineNode>>();

    let policy_node_id = SmolStr::new("policy:collision");
    let declared_artifact_ids = plan
        .artifacts
        .iter()
        .map(|artifact| artifact.id.clone())
        .collect::<BTreeSet<_>>();
    nodes.push(SpineNode {
        id: policy_node_id.clone(),
        family: SpineNodeFamily::PolicyRequirement,
        label: SmolStr::new("collision_policy"),
        query_contracts: Vec::new(),
        artifact_ids: Vec::new(),
        required_validity: None,
        notes: vec![
            SmolStr::new(format!("backend={}", dispatch_backend_name(plan.backend))),
            SmolStr::new(format!(
                "authority_scope={}",
                collision_authority_scope_name(plan.authority_scope)
            )),
            SmolStr::new(format!(
                "required_guarantee={}",
                plan.policy.required_guarantee.name()
            )),
            SmolStr::new(format!(
                "selected_method={}",
                plan.policy.selected_method.name()
            )),
        ],
    });

    nodes.extend(plan.passes.iter().map(|pass| {
        SpineNode {
            id: SmolStr::new(format!("invoke:{}", pass.id)),
            family: SpineNodeFamily::PrimitiveInvocation,
            label: SmolStr::new(collision_pass_spine_label(&pass.kind)),
            query_contracts: pass
                .kind
                .query_dependencies()
                .iter()
                .map(|contract| SmolStr::new(contract.as_str()))
                .collect(),
            artifact_ids: pass
                .consumes
                .iter()
                .chain(pass.materializes.iter())
                .filter(|value| declared_artifact_ids.contains(*value))
                .cloned()
                .collect(),
            required_validity: None,
            notes: vec![SmolStr::new(format!("pass_id={}", pass.id))],
        }
    }));

    nodes.extend(plan.artifacts.iter().map(|artifact| SpineNode {
        id: SmolStr::new(format!("artifact:{}", artifact.id)),
        family: SpineNodeFamily::ArtifactStore,
        label: SmolStr::new(collision_artifact_kind_name(artifact.kind)),
        query_contracts: Vec::new(),
        artifact_ids: vec![artifact.id.clone()],
        required_validity: Some(artifact.contract.validity.clone()),
        notes: vec![
            SmolStr::new(format!("contract_id={}", artifact.contract.id)),
            SmolStr::new(format!("record={}", artifact.record)),
        ],
    }));

    nodes.extend(
        artifact_uses
            .iter()
            .filter(|use_record| use_record.source == ArtifactUseSource::ArtifactStore)
            .map(|use_record| SpineNode {
                id: SmolStr::new(format!(
                    "load:{}:{}",
                    use_record.actor, use_record.artifact_id
                )),
                family: SpineNodeFamily::ArtifactLoad,
                label: use_record.artifact_id.clone(),
                query_contracts: Vec::new(),
                artifact_ids: vec![use_record.artifact_id.clone()],
                required_validity: use_record.required_validity.clone(),
                notes: vec![SmolStr::new(format!("actor={}", use_record.actor))],
            }),
    );

    nodes.extend(outputs.iter().map(spine_output_node));

    let observability = SpineObservabilitySummary {
        graph_structure: true,
        artifact_lifecycle: !artifact_uses.is_empty(),
        query_dependencies: plan
            .passes
            .iter()
            .any(|pass| !pass.kind.query_dependencies().is_empty()),
        backend_dispatch: true,
        output_bindings: !outputs.is_empty(),
        validation_summary: true,
        runtime_trace_local_only: true,
        observer_metrics_local_only: true,
    };
    nodes.push(SpineNode {
        id: SmolStr::new("observability:collision"),
        family: SpineNodeFamily::ObservabilitySummary,
        label: SmolStr::new("collision_observability"),
        query_contracts: Vec::new(),
        artifact_ids: Vec::new(),
        required_validity: None,
        notes: observability_notes(&observability),
    });

    let mut dependencies = Vec::new();
    let query_input_name = collision_query_input_name(plan);
    for pass in &plan.passes {
        let pass_node_id = SmolStr::new(format!("invoke:{}", pass.id));
        for input in collision_input_dependencies(&pass.kind, query_input_name.as_deref()) {
            dependencies.push(SpineDependencyEdge {
                from: SmolStr::new(format!("input:{input}")),
                to: pass_node_id.clone(),
                kind: SpineEdgeKind::ConsumesInput,
                subject: Some(SmolStr::new(input)),
                required_validity: None,
                lossy: false,
            });
        }
        dependencies.push(SpineDependencyEdge {
            from: policy_node_id.clone(),
            to: pass_node_id.clone(),
            kind: SpineEdgeKind::RequiresPolicy,
            subject: None,
            required_validity: None,
            lossy: false,
        });
        for use_record in artifact_uses
            .iter()
            .filter(|use_record| use_record.actor == pass.id)
        {
            let artifact_node_id = SmolStr::new(format!("artifact:{}", use_record.artifact_id));
            match (use_record.kind, use_record.source) {
                (ArtifactUseKind::Load, ArtifactUseSource::Plan) => {
                    dependencies.push(SpineDependencyEdge {
                        from: artifact_node_id,
                        to: pass_node_id.clone(),
                        kind: SpineEdgeKind::ConsumesArtifact,
                        subject: Some(use_record.artifact_id.clone()),
                        required_validity: None,
                        lossy: false,
                    });
                }
                (ArtifactUseKind::Load, ArtifactUseSource::ArtifactStore) => {
                    let load_node_id = SmolStr::new(format!(
                        "load:{}:{}",
                        use_record.actor, use_record.artifact_id
                    ));
                    dependencies.push(SpineDependencyEdge {
                        from: artifact_node_id.clone(),
                        to: load_node_id.clone(),
                        kind: SpineEdgeKind::LoadsArtifact,
                        subject: Some(use_record.artifact_id.clone()),
                        required_validity: use_record.required_validity.clone(),
                        lossy: false,
                    });
                    dependencies.push(SpineDependencyEdge {
                        from: load_node_id,
                        to: pass_node_id.clone(),
                        kind: SpineEdgeKind::ConsumesArtifact,
                        subject: Some(use_record.artifact_id.clone()),
                        required_validity: use_record.required_validity.clone(),
                        lossy: false,
                    });
                }
                (ArtifactUseKind::Produce, ArtifactUseSource::Plan) => {
                    dependencies.push(SpineDependencyEdge {
                        from: pass_node_id.clone(),
                        to: artifact_node_id,
                        kind: SpineEdgeKind::ProducesArtifact,
                        subject: Some(use_record.artifact_id.clone()),
                        required_validity: None,
                        lossy: false,
                    });
                }
                (ArtifactUseKind::Preserve, ArtifactUseSource::Plan) => {
                    dependencies.push(SpineDependencyEdge {
                        from: pass_node_id.clone(),
                        to: artifact_node_id,
                        kind: SpineEdgeKind::StoresArtifact,
                        subject: Some(use_record.artifact_id.clone()),
                        required_validity: use_record.required_validity.clone(),
                        lossy: false,
                    });
                }
                (ArtifactUseKind::Produce, ArtifactUseSource::ArtifactStore)
                | (ArtifactUseKind::Preserve, ArtifactUseSource::ArtifactStore) => {}
            }
        }
    }
    let mut value_producers = BTreeMap::new();
    for pass in &plan.passes {
        if matches!(pass.kind, CollisionPassKind::MaterializeOutput { .. }) {
            continue;
        }
        for value in pass
            .materializes
            .iter()
            .filter(|value| !declared_artifact_ids.contains(*value))
        {
            value_producers
                .entry(value.clone())
                .or_insert_with(|| pass.id.clone());
        }
    }
    for pass in &plan.passes {
        let pass_node_id = SmolStr::new(format!("invoke:{}", pass.id));
        for value in pass
            .consumes
            .iter()
            .filter(|value| !declared_artifact_ids.contains(*value))
        {
            if let Some(producer) = value_producers.get(value) {
                dependencies.push(SpineDependencyEdge {
                    from: SmolStr::new(format!("invoke:{}", producer)),
                    to: pass_node_id.clone(),
                    kind: SpineEdgeKind::ConsumesValue,
                    subject: Some(value.clone()),
                    required_validity: None,
                    lossy: false,
                });
            }
        }
    }
    for output in &plan.outputs {
        if let Some(pass) = plan.passes.iter().find(|pass| {
            matches!(
                pass.kind,
                CollisionPassKind::MaterializeOutput { output: kind } if kind == output.kind
            )
        }) {
            dependencies.push(SpineDependencyEdge {
                from: SmolStr::new(format!("invoke:{}", pass.id)),
                to: SmolStr::new(format!("output:{}", output.name)),
                kind: SpineEdgeKind::FeedsOutput,
                subject: Some(output.name.clone()),
                required_validity: None,
                lossy: false,
            });
        }
    }

    let lossy_boundaries = vec![
        SpineLossyBoundary {
            node_id: SmolStr::new("projection:collision"),
            reason: SpineLossyReason::ObserverSpecificSemantics,
            dropped_fields: vec![
                SmolStr::new("collision_contract_descriptor"),
                SmolStr::new("witness_payload_semantics"),
            ],
        },
        SpineLossyBoundary {
            node_id: policy_node_id.clone(),
            reason: SpineLossyReason::PolicyAuthority,
            dropped_fields: vec![
                SmolStr::new("full_authority_contract"),
                SmolStr::new("backend_legality_checks"),
            ],
        },
        SpineLossyBoundary {
            node_id: SmolStr::new("observability:collision"),
            reason: SpineLossyReason::RuntimeTrace,
            dropped_fields: vec![
                SmolStr::new("artifact_store"),
                SmolStr::new("reuse_metrics"),
                SmolStr::new("reuse_decisions"),
            ],
        },
    ];

    ObserverProjection {
        observer_kind: ObserverKind::Collision,
        source_plan: plan.name.clone(),
        execution_owner: SmolStr::new("CollisionPlan"),
        observer_local_notes: vec![
            SmolStr::new("collision plans remain the execution owners"),
            SmolStr::new(
                "runtime traces, authority semantics, and witness evaluation remain local",
            ),
        ],
        lossy_boundaries: lossy_boundaries.clone(),
        spine: QueryProgramSpine {
            schema_version: QUERY_PROGRAM_SPINE_SCHEMA_VERSION,
            observer_kind: ObserverKind::Collision,
            source_plan: plan.name.clone(),
            inputs,
            nodes,
            dependencies,
            outputs,
            semantic_artifacts,
            artifact_uses,
            observability,
        },
    }
}

fn spine_input_node(binding: &SpineInputBinding) -> SpineNode {
    SpineNode {
        id: binding.node_id.clone(),
        family: SpineNodeFamily::InputBinding,
        label: binding.binding.clone(),
        query_contracts: Vec::new(),
        artifact_ids: Vec::new(),
        required_validity: None,
        notes: binding.notes.clone(),
    }
}

fn spine_output_node(binding: &SpineOutputBinding) -> SpineNode {
    SpineNode {
        id: binding.node_id.clone(),
        family: SpineNodeFamily::OutputBinding,
        label: binding.binding.clone(),
        query_contracts: Vec::new(),
        artifact_ids: Vec::new(),
        required_validity: None,
        notes: binding.notes.clone(),
    }
}

fn primitive_presentation_notes(pass: &crate::presentation_plan::PresentationPass) -> Vec<SmolStr> {
    let mut notes = vec![SmolStr::new(format!("pass_id={}", pass.id))];
    if let Some(binding) = pass.binding.as_ref() {
        notes.push(SmolStr::new(format!("binding={}", binding.as_str())));
    }
    if !pass.future_acceleration_hooks.is_empty() {
        notes.push(SmolStr::new(format!(
            "future_acceleration_hooks={}",
            pass.future_acceleration_hooks.len()
        )));
    }
    notes
}

fn observability_notes(summary: &SpineObservabilitySummary) -> Vec<SmolStr> {
    vec![
        SmolStr::new(format!("graph_structure={}", summary.graph_structure)),
        SmolStr::new(format!("artifact_lifecycle={}", summary.artifact_lifecycle)),
        SmolStr::new(format!("query_dependencies={}", summary.query_dependencies)),
        SmolStr::new(format!("backend_dispatch={}", summary.backend_dispatch)),
        SmolStr::new(format!("output_bindings={}", summary.output_bindings)),
        SmolStr::new(format!("validation_summary={}", summary.validation_summary)),
        SmolStr::new(format!(
            "runtime_trace_local_only={}",
            summary.runtime_trace_local_only
        )),
        SmolStr::new(format!(
            "observer_metrics_local_only={}",
            summary.observer_metrics_local_only
        )),
    ]
}

fn presentation_input_dependencies(kind: &PresentationPassKind) -> &'static [&'static str] {
    match kind {
        PresentationPassKind::PrimaryVisibility { .. } => &["world", "camera"],
        PresentationPassKind::SurfaceResolve { .. } => &["world"],
        PresentationPassKind::ParticipantsResolve { .. } => &["world"],
        PresentationPassKind::WorldBatchQuery { .. } => &["world"],
        PresentationPassKind::GenerateScreenSamples { .. }
        | PresentationPassKind::ShadePrimary { .. }
        | PresentationPassKind::CompositeColor { .. }
        | PresentationPassKind::MotionResolve { .. }
        | PresentationPassKind::TemporalResolve { .. }
        | PresentationPassKind::KernelDispatch
        | PresentationPassKind::ExportAttachment { .. } => &[],
    }
}

fn collision_query_input_name(plan: &CollisionPlan) -> Option<String> {
    plan.inputs
        .iter()
        .find(|input| input.name != "world" && input.name != "domain" && input.name != "transition")
        .map(|input| input.name.to_string())
}

fn collision_input_dependencies(
    kind: &CollisionPassKind,
    query_input_name: Option<&str>,
) -> Vec<String> {
    let mut inputs = Vec::new();
    match kind {
        CollisionPassKind::GatherCandidates { .. } => {
            inputs.extend([String::from("world"), String::from("domain")]);
            if let Some(query_input_name) = query_input_name {
                inputs.push(query_input_name.to_string());
            }
        }
        CollisionPassKind::EvaluatePointOccupancy { .. }
        | CollisionPassKind::CastRayFirstHit { .. }
        | CollisionPassKind::ResolveSphereOverlap { .. } => {
            inputs.extend([String::from("world"), String::from("domain")]);
            if let Some(query_input_name) = query_input_name {
                inputs.push(query_input_name.to_string());
            }
        }
        CollisionPassKind::SweepSphereFirstContact { .. }
        | CollisionPassKind::ResolveSphereTimeOfImpact { .. } => {
            inputs.extend([
                String::from("world"),
                String::from("domain"),
                String::from("transition"),
            ]);
            if let Some(query_input_name) = query_input_name {
                inputs.push(query_input_name.to_string());
            }
        }
        CollisionPassKind::BuildBroadphaseCandidates { .. }
        | CollisionPassKind::MaterializeOutput { .. } => {}
    }
    inputs
}

fn join_display<'a>(values: impl IntoIterator<Item = &'a str>) -> String {
    values.into_iter().collect::<Vec<_>>().join(",")
}

fn presentation_pass_spine_label(kind: &PresentationPassKind) -> &'static str {
    match kind {
        PresentationPassKind::GenerateScreenSamples { .. } => "generate_screen_samples",
        PresentationPassKind::PrimaryVisibility { .. } => "primary_visibility",
        PresentationPassKind::SurfaceResolve { .. } => "surface_resolve",
        PresentationPassKind::ParticipantsResolve { .. } => "participants_resolve",
        PresentationPassKind::ShadePrimary { .. } => "shade_primary",
        PresentationPassKind::CompositeColor { .. } => "composite_color",
        PresentationPassKind::MotionResolve { .. } => "motion_resolve",
        PresentationPassKind::TemporalResolve { .. } => "temporal_resolve",
        PresentationPassKind::WorldBatchQuery { .. } => "world_batch_query",
        PresentationPassKind::KernelDispatch => "kernel_dispatch",
        PresentationPassKind::ExportAttachment { .. } => "export_attachment",
    }
}

fn collision_pass_spine_label(kind: &CollisionPassKind) -> &'static str {
    match kind {
        CollisionPassKind::GatherCandidates { .. } => "gather_candidates",
        CollisionPassKind::BuildBroadphaseCandidates { .. } => "build_broadphase_candidates",
        CollisionPassKind::EvaluatePointOccupancy { .. } => "evaluate_point_occupancy",
        CollisionPassKind::CastRayFirstHit { .. } => "cast_ray_first_hit",
        CollisionPassKind::ResolveSphereOverlap { .. } => "resolve_sphere_overlap",
        CollisionPassKind::SweepSphereFirstContact { .. } => "sweep_sphere_first_contact",
        CollisionPassKind::ResolveSphereTimeOfImpact { .. } => "resolve_sphere_time_of_impact",
        CollisionPassKind::MaterializeOutput { .. } => "materialize_output",
    }
}

fn frame_attachment_kind_name(kind: FrameAttachmentKind) -> &'static str {
    match kind {
        FrameAttachmentKind::PrimaryHit => "primary_hit",
        FrameAttachmentKind::Depth => "depth",
        FrameAttachmentKind::WorldNormal => "world_normal",
        FrameAttachmentKind::Surface => "surface",
        FrameAttachmentKind::Radiance => "radiance",
        FrameAttachmentKind::Medium => "medium",
        FrameAttachmentKind::Motion => "motion",
        FrameAttachmentKind::Color => "color",
    }
}

fn attachment_lifetime_name(lifetime: AttachmentLifetime) -> String {
    match lifetime {
        AttachmentLifetime::Transient => "transient".to_string(),
        AttachmentLifetime::Exported => "exported".to_string(),
        AttachmentLifetime::HistorySlot(slot) => format!("history_slot({slot})"),
    }
}

fn attachment_resolution_name(resolution: AttachmentResolutionClass) -> &'static str {
    match resolution {
        AttachmentResolutionClass::Viewport => "viewport",
        AttachmentResolutionClass::HalfViewport => "half_viewport",
        AttachmentResolutionClass::QuarterViewport => "quarter_viewport",
    }
}

fn attachment_resolution_scale_name(scale: AttachmentResolutionScale) -> String {
    format!("{}/{}", scale.divisor_x, scale.divisor_y)
}

fn attachment_clear_policy_name(clear_policy: AttachmentClearPolicy) -> &'static str {
    match clear_policy {
        AttachmentClearPolicy::Zero => "zero",
        AttachmentClearPolicy::SemanticDefault => "semantic_default",
        AttachmentClearPolicy::PreservePrevious => "preserve_previous",
    }
}

fn attachment_element_schema_name(schema: &AttachmentElementSchema) -> String {
    match schema {
        AttachmentElementSchema::NamedRecord(name) => name.to_string(),
        AttachmentElementSchema::ScalarF32 => "f32".to_string(),
        AttachmentElementSchema::Vec2F32 => "vec2<f32>".to_string(),
        AttachmentElementSchema::Vec3F32 => "vec3<f32>".to_string(),
        AttachmentElementSchema::Vec4F32 => "vec4<f32>".to_string(),
    }
}

fn canonical_projection_input_name(input: CanonicalProjectionInput) -> &'static str {
    match input {
        CanonicalProjectionInput::CameraVerticalFovDegrees => "camera.vertical_fov_degrees",
    }
}

fn view_ray_space_name(space: CanonicalViewRaySpace) -> &'static str {
    match space {
        CanonicalViewRaySpace::World => "world",
    }
}

fn dispatch_backend_name(backend: crate::query_plan::DispatchBackend) -> &'static str {
    match backend {
        crate::query_plan::DispatchBackend::Cpu => "cpu",
        crate::query_plan::DispatchBackend::VirtualGpu => "virtual_gpu",
        crate::query_plan::DispatchBackend::Wgsl => "wgsl",
        crate::query_plan::DispatchBackend::Auto => "auto",
    }
}
