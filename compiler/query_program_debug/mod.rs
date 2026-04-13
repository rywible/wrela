use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NormalizedCurrentPlanProjection {
    pub schema_version: u32,
    pub source_plan: String,
    pub family: String,
    pub execution_mode: String,
    pub pass_kinds: Vec<String>,
    pub query_contracts: Vec<String>,
    pub frame_artifacts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ObserverProjectionDump {
    pub observer_kind: String,
    pub source_plan: String,
    pub execution_owner: String,
    pub observer_local_notes: Vec<String>,
    pub lossy_boundaries: Vec<SpineLossyBoundaryDump>,
    pub spine: QueryProgramSpineDump,
    pub analysis: SharedSpineReportDump,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QueryProgramSpineDump {
    pub schema_version: u32,
    pub observer_kind: String,
    pub source_plan: String,
    pub inputs: Vec<SpineBindingDump>,
    pub nodes: Vec<SpineNodeDump>,
    pub dependencies: Vec<SpineDependencyEdgeDump>,
    pub outputs: Vec<SpineBindingDump>,
    pub semantic_artifacts: Vec<SpineSemanticArtifactDump>,
    pub artifact_uses: Vec<SpineArtifactUseDump>,
    pub observability: SpineObservabilitySummaryDump,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpineBindingDump {
    pub node_id: String,
    pub binding: String,
    pub schema: String,
    pub role: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpineNodeDump {
    pub id: String,
    pub family: String,
    pub label: String,
    pub query_contracts: Vec<String>,
    pub artifact_ids: Vec<String>,
    pub required_validity: Option<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpineDependencyEdgeDump {
    pub from: String,
    pub to: String,
    pub kind: String,
    pub subject: Option<String>,
    pub required_validity: Option<String>,
    pub lossy: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpineLossyBoundaryDump {
    pub node_id: String,
    pub reason: String,
    pub dropped_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpineSemanticArtifactDump {
    pub id: String,
    pub kind: String,
    pub logical_schema: String,
    pub snapshot_relation: String,
    pub validity: String,
    pub producer: String,
    pub consumer: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpineArtifactUseDump {
    pub actor: String,
    pub artifact_id: String,
    pub kind: String,
    pub source: String,
    pub required_validity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpineObservabilitySummaryDump {
    pub graph_structure: bool,
    pub artifact_lifecycle: bool,
    pub query_dependencies: bool,
    pub backend_dispatch: bool,
    pub output_bindings: bool,
    pub validation_summary: bool,
    pub runtime_trace_local_only: bool,
    pub observer_metrics_local_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SharedSpineReportDump {
    pub dependency: SpineDependencyAnalysisDump,
    pub artifact_lifetimes: SpineArtifactLifetimeValidationDump,
    pub policy: SpinePolicyLegalitySummaryDump,
    pub backend: SpineBackendSummaryDump,
    pub observability: SpineObservabilityReportDump,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SharedSpineIssueDump {
    pub scope: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpineDependencyAnalysisDump {
    pub status: String,
    pub node_count: usize,
    pub edge_count: usize,
    pub root_nodes: Vec<String>,
    pub leaf_nodes: Vec<String>,
    pub topological_order: Vec<String>,
    pub cycle_nodes: Vec<String>,
    pub artifact_edge_count: usize,
    pub policy_edge_count: usize,
    pub output_edge_count: usize,
    pub issues: Vec<SharedSpineIssueDump>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpineArtifactAccessSummaryDump {
    pub actor: String,
    pub artifact_id: String,
    pub required_validity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpineArtifactLifetimeValidationDump {
    pub status: String,
    pub explicit_artifacts: Vec<String>,
    pub store_backed_loads: Vec<SpineArtifactAccessSummaryDump>,
    pub preserved_artifacts: Vec<String>,
    pub issues: Vec<SharedSpineIssueDump>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpinePolicyRequirementSummaryDump {
    pub node_id: String,
    pub label: String,
    pub backends: Vec<String>,
    pub supported_backends: Vec<String>,
    pub backend_preference: Option<String>,
    pub required_guarantee: Option<String>,
    pub selected_method: Option<String>,
    pub authority_scope: Option<String>,
    pub legal: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpinePolicyLegalitySummaryDump {
    pub status: String,
    pub requirements: Vec<SpinePolicyRequirementSummaryDump>,
    pub issues: Vec<SharedSpineIssueDump>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpineBackendSummaryDump {
    pub status: String,
    pub active_backends: Vec<String>,
    pub supported_backends: Vec<String>,
    pub binding_count: Option<u32>,
    pub dispatch_nodes: Vec<String>,
    pub backend_dispatch_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpineObservabilityBoundaryReportDump {
    pub node_id: String,
    pub reason: String,
    pub dropped_field_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpineObservabilityReportDump {
    pub common_channels: Vec<String>,
    pub local_only_channels: Vec<String>,
    pub lossy_boundaries: Vec<SpineObservabilityBoundaryReportDump>,
}

pub fn projection_for_presentation_plan(
    plan: &wrela::presentation_plan::PresentationPlan,
) -> NormalizedCurrentPlanProjection {
    let mut query_contracts = BTreeSet::<String>::new();
    let mut pass_kinds = Vec::new();
    for pass in &plan.passes {
        pass_kinds.push(presentation_pass_kind_name(&pass.kind).to_string());
        for contract_id in &pass.query_dependencies {
            query_contracts.insert(contract_id.as_str().to_string());
        }
    }

    NormalizedCurrentPlanProjection {
        schema_version: 1,
        source_plan: plan.name.to_string(),
        family: "presentation".to_string(),
        execution_mode: if plan.passes.iter().any(|pass| {
            matches!(
                pass.kind,
                wrela::presentation_plan::PresentationPassKind::MotionResolve { .. }
                    | wrela::presentation_plan::PresentationPassKind::TemporalResolve { .. }
            )
        }) {
            "temporal".to_string()
        } else {
            "composite".to_string()
        },
        pass_kinds,
        query_contracts: query_contracts.into_iter().collect(),
        frame_artifacts: plan
            .frame_artifacts
            .iter()
            .map(|artifact| artifact.attachment.to_string())
            .collect(),
    }
}

pub fn observer_projection_for_presentation_plan(
    plan: &wrela::presentation_plan::PresentationPlan,
) -> ObserverProjectionDump {
    observer_projection_dump(wrela::query_program_spine::project_presentation_plan(plan))
}

pub fn observer_projection_for_collision_plan(
    plan: &wrela::collision_plan::CollisionPlan,
) -> ObserverProjectionDump {
    observer_projection_dump(wrela::query_program_spine::project_collision_plan(plan))
}

fn observer_projection_dump(
    projection: wrela::query_program_spine::ObserverProjection,
) -> ObserverProjectionDump {
    let analysis = wrela::query_program_spine::shared_spine_report(&projection);
    ObserverProjectionDump {
        observer_kind: wrela::query_program_spine::observer_kind_name(projection.observer_kind)
            .to_string(),
        source_plan: projection.source_plan.to_string(),
        execution_owner: projection.execution_owner.to_string(),
        observer_local_notes: projection
            .observer_local_notes
            .into_iter()
            .map(|note| note.to_string())
            .collect(),
        lossy_boundaries: projection
            .lossy_boundaries
            .into_iter()
            .map(|boundary| SpineLossyBoundaryDump {
                node_id: boundary.node_id.to_string(),
                reason: wrela::query_program_spine::spine_lossy_reason_name(boundary.reason)
                    .to_string(),
                dropped_fields: boundary
                    .dropped_fields
                    .into_iter()
                    .map(|field| field.to_string())
                    .collect(),
            })
            .collect(),
        spine: query_program_spine_dump(projection.spine),
        analysis: shared_spine_report_dump(analysis),
    }
}

fn query_program_spine_dump(
    spine: wrela::query_program_spine::QueryProgramSpine,
) -> QueryProgramSpineDump {
    QueryProgramSpineDump {
        schema_version: spine.schema_version,
        observer_kind: wrela::query_program_spine::observer_kind_name(spine.observer_kind)
            .to_string(),
        source_plan: spine.source_plan.to_string(),
        inputs: spine
            .inputs
            .into_iter()
            .map(|binding| SpineBindingDump {
                node_id: binding.node_id.to_string(),
                binding: binding.binding.to_string(),
                schema: binding.schema.to_string(),
                role: binding.role.to_string(),
                notes: binding
                    .notes
                    .into_iter()
                    .map(|note| note.to_string())
                    .collect(),
            })
            .collect(),
        nodes: spine
            .nodes
            .into_iter()
            .map(|node| SpineNodeDump {
                id: node.id.to_string(),
                family: wrela::query_program_spine::spine_node_family_name(node.family).to_string(),
                label: node.label.to_string(),
                query_contracts: node
                    .query_contracts
                    .into_iter()
                    .map(|contract| contract.to_string())
                    .collect(),
                artifact_ids: node
                    .artifact_ids
                    .into_iter()
                    .map(|artifact_id| artifact_id.to_string())
                    .collect(),
                required_validity: node
                    .required_validity
                    .map(|validity| format!("{validity:?}")),
                notes: node
                    .notes
                    .into_iter()
                    .map(|note| note.to_string())
                    .collect(),
            })
            .collect(),
        dependencies: spine
            .dependencies
            .into_iter()
            .map(|edge| SpineDependencyEdgeDump {
                from: edge.from.to_string(),
                to: edge.to.to_string(),
                kind: wrela::query_program_spine::spine_edge_kind_name(edge.kind).to_string(),
                subject: edge.subject.map(|subject| subject.to_string()),
                required_validity: edge
                    .required_validity
                    .map(|validity| format!("{validity:?}")),
                lossy: edge.lossy,
            })
            .collect(),
        outputs: spine
            .outputs
            .into_iter()
            .map(|binding| SpineBindingDump {
                node_id: binding.node_id.to_string(),
                binding: binding.binding.to_string(),
                schema: binding.schema.to_string(),
                role: binding.role.to_string(),
                notes: binding
                    .notes
                    .into_iter()
                    .map(|note| note.to_string())
                    .collect(),
            })
            .collect(),
        semantic_artifacts: spine
            .semantic_artifacts
            .into_iter()
            .map(|artifact| SpineSemanticArtifactDump {
                id: artifact.id.to_string(),
                kind: format!("{:?}", artifact.kind),
                logical_schema: artifact.logical_schema.describe().to_string(),
                snapshot_relation: wrela::artifact_contract::snapshot_relation_name(
                    artifact.compatibility.snapshot,
                )
                .to_string(),
                validity: format!("{:?}", artifact.validity),
                producer: artifact.producer.to_string(),
                consumer: artifact.consumer.to_string(),
            })
            .collect(),
        artifact_uses: spine
            .artifact_uses
            .into_iter()
            .map(|use_record| SpineArtifactUseDump {
                actor: use_record.actor.to_string(),
                artifact_id: use_record.artifact_id.to_string(),
                kind: wrela::artifact_contract::artifact_use_kind_name(use_record.kind).to_string(),
                source: wrela::artifact_contract::artifact_use_source_name(use_record.source)
                    .to_string(),
                required_validity: use_record
                    .required_validity
                    .map(|validity| format!("{validity:?}")),
            })
            .collect(),
        observability: SpineObservabilitySummaryDump {
            graph_structure: spine.observability.graph_structure,
            artifact_lifecycle: spine.observability.artifact_lifecycle,
            query_dependencies: spine.observability.query_dependencies,
            backend_dispatch: spine.observability.backend_dispatch,
            output_bindings: spine.observability.output_bindings,
            validation_summary: spine.observability.validation_summary,
            runtime_trace_local_only: spine.observability.runtime_trace_local_only,
            observer_metrics_local_only: spine.observability.observer_metrics_local_only,
        },
    }
}

fn shared_spine_report_dump(
    report: wrela::query_program_spine::SharedSpineReport,
) -> SharedSpineReportDump {
    let explicit_artifacts = report
        .artifact_lifetime
        .contract_checks
        .iter()
        .filter(|check| check.expected_validity.is_explicit())
        .map(|check| check.artifact_id.clone())
        .collect::<Vec<_>>();
    let store_backed_loads = report
        .artifact_lifetime
        .use_checks
        .iter()
        .filter(|check| {
            check.kind == wrela::artifact_contract::ArtifactUseKind::Load
                && check.source == wrela::artifact_contract::ArtifactUseSource::ArtifactStore
        })
        .map(|check| SpineArtifactAccessSummaryDump {
            actor: check.actor.to_string(),
            artifact_id: check.artifact_id.to_string(),
            required_validity: check
                .required_validity
                .as_ref()
                .map(|validity| format!("{validity:?}")),
        })
        .collect::<Vec<_>>();
    let preserved_artifacts = report
        .artifact_lifetime
        .use_checks
        .iter()
        .filter(|check| check.kind == wrela::artifact_contract::ArtifactUseKind::Preserve)
        .map(|check| check.artifact_id.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let dependency_cycle_issues = (report.dependency.status
        == wrela::query_program_spine::SpineAnalysisStatus::Invalid)
        .then(|| {
            report
                .dependency
                .cycles
                .clone()
                .into_iter()
                .map(wrela::query_program_spine::SharedSpineIssue::Cycle)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let artifact_issues = report
        .artifact_lifetime
        .contract_checks
        .iter()
        .filter(|check| check.status != wrela::query_program_spine::SpineAnalysisStatus::Valid)
        .cloned()
        .map(wrela::query_program_spine::SharedSpineIssue::ArtifactContract)
        .chain(
            report
                .artifact_lifetime
                .use_checks
                .iter()
                .filter(|check| {
                    check.status != wrela::query_program_spine::SpineAnalysisStatus::Valid
                })
                .cloned()
                .map(wrela::query_program_spine::SharedSpineIssue::ArtifactUse),
        )
        .filter(|issue| !shared_spine_issue_dump(issue.clone()).message.is_empty())
        .collect::<Vec<_>>();
    let policy_issues = report
        .policy
        .policy_nodes
        .iter()
        .filter(|node| node.status != wrela::query_program_spine::SpineAnalysisStatus::Valid)
        .cloned()
        .map(wrela::query_program_spine::SharedSpineIssue::PolicyRequirement)
        .map(shared_spine_issue_dump)
        .filter(|issue| !issue.message.is_empty())
        .chain(
            report
                .policy
                .illegal_combinations
                .iter()
                .map(|message| SharedSpineIssueDump {
                    scope: "policy".to_string(),
                    message: message.to_string(),
                }),
        )
        .collect::<Vec<_>>();

    SharedSpineReportDump {
        dependency: SpineDependencyAnalysisDump {
            status: wrela::query_program_spine::spine_analysis_status_name(
                report.dependency.status,
            )
            .to_string(),
            node_count: report.dependency.node_count,
            edge_count: report.dependency.edge_count,
            root_nodes: report
                .dependency
                .roots
                .into_iter()
                .map(|node_id| node_id.to_string())
                .collect(),
            leaf_nodes: report
                .dependency
                .leaves
                .into_iter()
                .map(|node_id| node_id.to_string())
                .collect(),
            topological_order: report
                .dependency
                .topological_order
                .into_iter()
                .map(|node_id| node_id.to_string())
                .collect(),
            cycle_nodes: report
                .dependency
                .cycles
                .iter()
                .cloned()
                .flat_map(|cycle| cycle.nodes.into_iter().map(|node_id| node_id.to_string()))
                .collect(),
            artifact_edge_count: report.dependency.artifact_edge_count,
            policy_edge_count: report.dependency.policy_edge_count,
            output_edge_count: report.dependency.output_edge_count,
            issues: report
                .dependency
                .missing_node_edges
                .iter()
                .cloned()
                .map(wrela::query_program_spine::SharedSpineIssue::MissingNodeEdge)
                .chain(dependency_cycle_issues.into_iter())
                .map(shared_spine_issue_dump)
                .filter(|issue| !issue.message.is_empty())
                .collect(),
        },
        artifact_lifetimes: SpineArtifactLifetimeValidationDump {
            status: wrela::query_program_spine::spine_analysis_status_name(
                report.artifact_lifetime.status,
            )
            .to_string(),
            explicit_artifacts: explicit_artifacts
                .into_iter()
                .map(|artifact_id| artifact_id.to_string())
                .collect(),
            store_backed_loads,
            preserved_artifacts: preserved_artifacts
                .into_iter()
                .map(|artifact_id| artifact_id.to_string())
                .collect(),
            issues: artifact_issues
                .into_iter()
                .map(shared_spine_issue_dump)
                .collect(),
        },
        policy: SpinePolicyLegalitySummaryDump {
            status: wrela::query_program_spine::spine_analysis_status_name(report.policy.status)
                .to_string(),
            requirements: report
                .policy
                .policy_nodes
                .iter()
                .cloned()
                .map(|requirement| SpinePolicyRequirementSummaryDump {
                    node_id: requirement.node_id.to_string(),
                    label: requirement.label.to_string(),
                    backends: requirement
                        .backend
                        .into_iter()
                        .map(|backend| {
                            wrela::query_program_spine::spine_backend_name(&backend).to_string()
                        })
                        .collect(),
                    supported_backends: requirement
                        .supported_backends
                        .into_iter()
                        .map(|backend| {
                            wrela::query_program_spine::spine_backend_name(&backend).to_string()
                        })
                        .collect(),
                    backend_preference: requirement.backend_preference.map(|backend| {
                        wrela::query_program_spine::spine_backend_name(&backend).to_string()
                    }),
                    required_guarantee: requirement
                        .required_guarantee
                        .map(|guarantee| guarantee.to_string()),
                    selected_method: requirement.selected_method.map(|method| method.to_string()),
                    authority_scope: requirement.authority_scope.map(|scope| scope.to_string()),
                    legal: requirement.status
                        != wrela::query_program_spine::SpineAnalysisStatus::Invalid
                        && !report
                            .policy
                            .illegal_combinations
                            .iter()
                            .any(|issue| issue.starts_with(&format!("{}:", requirement.node_id))),
                })
                .collect(),
            issues: policy_issues,
        },
        backend: SpineBackendSummaryDump {
            status: wrela::query_program_spine::spine_analysis_status_name(report.backend.status)
                .to_string(),
            active_backends: report
                .backend
                .explicit_backends
                .into_iter()
                .map(|backend| wrela::query_program_spine::spine_backend_name(&backend).to_string())
                .collect(),
            supported_backends: report
                .backend
                .supported_backends
                .into_iter()
                .map(|backend| wrela::query_program_spine::spine_backend_name(&backend).to_string())
                .collect(),
            binding_count: report.backend.binding_count,
            dispatch_nodes: report
                .policy
                .policy_nodes
                .iter()
                .map(|node| node.node_id.to_string())
                .collect(),
            backend_dispatch_enabled: report.backend.dispatch_observable,
        },
        observability: SpineObservabilityReportDump {
            common_channels: shared_observability_channels(&report.observability.summary),
            local_only_channels: shared_local_only_channels(&report.observability.local_only),
            lossy_boundaries: report
                .observability
                .lossy_boundaries
                .into_iter()
                .map(|boundary| SpineObservabilityBoundaryReportDump {
                    node_id: boundary.node_id.to_string(),
                    reason: boundary.reason.to_string(),
                    dropped_field_count: boundary.dropped_fields.len(),
                })
                .collect(),
        },
    }
}

fn shared_spine_issue_dump(
    issue: wrela::query_program_spine::SharedSpineIssue,
) -> SharedSpineIssueDump {
    match issue {
        wrela::query_program_spine::SharedSpineIssue::MissingNodeEdge(edge) => {
            SharedSpineIssueDump {
                scope: format!("edge:{}->{}", edge.from, edge.to),
                message: format!(
                    "missing_node_edge kind={} missing_from={} missing_to={}",
                    wrela::query_program_spine::spine_edge_kind_name(edge.kind),
                    edge.missing_from,
                    edge.missing_to
                ),
            }
        }
        wrela::query_program_spine::SharedSpineIssue::Cycle(cycle) => SharedSpineIssueDump {
            scope: "dependency_cycle".to_string(),
            message: format!(
                "cycle nodes={} self_loop={}",
                cycle
                    .nodes
                    .iter()
                    .map(|node_id| node_id.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                cycle.self_loop
            ),
        },
        wrela::query_program_spine::SharedSpineIssue::ArtifactContract(check) => {
            SharedSpineIssueDump {
                scope: check.artifact_id.to_string(),
                message: check
                    .notes
                    .iter()
                    .map(|note| note.as_str())
                    .collect::<Vec<_>>()
                    .join("; "),
            }
        }
        wrela::query_program_spine::SharedSpineIssue::ArtifactUse(check) => SharedSpineIssueDump {
            scope: format!("{}:{}", check.actor, check.artifact_id),
            message: check
                .notes
                .iter()
                .map(|note| note.as_str())
                .collect::<Vec<_>>()
                .join("; "),
        },
        wrela::query_program_spine::SharedSpineIssue::PolicyRequirement(node) => {
            SharedSpineIssueDump {
                scope: node.node_id.to_string(),
                message: node
                    .notes
                    .iter()
                    .map(|note| note.as_str())
                    .collect::<Vec<_>>()
                    .join("; "),
            }
        }
    }
}

fn shared_observability_channels(
    summary: &wrela::query_program_spine::SpineObservabilitySummary,
) -> Vec<String> {
    let mut channels = Vec::new();
    if summary.graph_structure {
        channels.push("graph_structure".to_string());
    }
    if summary.artifact_lifecycle {
        channels.push("artifact_lifecycle".to_string());
    }
    if summary.query_dependencies {
        channels.push("query_dependencies".to_string());
    }
    if summary.backend_dispatch {
        channels.push("backend_dispatch".to_string());
    }
    if summary.output_bindings {
        channels.push("output_bindings".to_string());
    }
    if summary.validation_summary {
        channels.push("validation_summary".to_string());
    }
    channels
}

fn shared_local_only_channels(
    local_only: &wrela::query_program_spine::report::SpineLocalOnlyFlags,
) -> Vec<String> {
    let mut channels = Vec::new();
    if local_only.runtime_trace_local_only {
        channels.push("runtime_trace".to_string());
    }
    if local_only.observer_metrics_local_only {
        channels.push("observer_metrics".to_string());
    }
    channels
}

fn presentation_pass_kind_name(
    kind: &wrela::presentation_plan::PresentationPassKind,
) -> &'static str {
    match kind {
        wrela::presentation_plan::PresentationPassKind::GenerateScreenSamples { .. } => {
            "generate_screen_samples"
        }
        wrela::presentation_plan::PresentationPassKind::PrimaryVisibility { .. } => {
            "primary_visibility"
        }
        wrela::presentation_plan::PresentationPassKind::SurfaceResolve { .. } => "surface_resolve",
        wrela::presentation_plan::PresentationPassKind::ParticipantsResolve { .. } => {
            "participants_resolve"
        }
        wrela::presentation_plan::PresentationPassKind::ShadePrimary { .. } => "shade_primary",
        wrela::presentation_plan::PresentationPassKind::CompositeColor { .. } => "composite_color",
        wrela::presentation_plan::PresentationPassKind::MotionResolve { .. } => "motion_resolve",
        wrela::presentation_plan::PresentationPassKind::TemporalResolve { .. } => {
            "temporal_resolve"
        }
        wrela::presentation_plan::PresentationPassKind::WorldBatchQuery { .. } => {
            "world_batch_query"
        }
        wrela::presentation_plan::PresentationPassKind::KernelDispatch => "kernel_dispatch",
        wrela::presentation_plan::PresentationPassKind::ExportAttachment { .. } => {
            "export_attachment"
        }
    }
}
