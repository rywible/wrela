//! Owns human/JSON projection helpers for observer, artifact, and shared-state
//! diagnostics emitted by CLI command reports.
//! Does not own observer analysis or the command orchestration that chooses when
//! these projections are rendered.
//!
//! Key invariants:
//! - projection helpers must preserve the observer/runtime relationships from
//!   the source diagnostic model.
//! - shared-node and spine formatting may reorder for readability, but they must
//!   not invent or drop semantic edges.
//! - human-readable helpers and serialized dump structs describe the same
//!   observer state.
//!
//! Primary entrypoints:
//! - `observer_semantic_artifact_dump`
//! - `observer_validation_summary`
//! - `print_observer_projection_human`
//!
//! Failure modes / common pitfalls:
//! - hiding missing observer edges in formatting code makes debugging artifact
//!   wiring much harder.
//! - letting command-specific text leak into these helpers couples reports that
//!   should stay reusable.

use super::*;

#[derive(Serialize)]
pub(crate) struct ObserverSemanticArtifactDump {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) logical_schema: String,
    pub(crate) snapshot_relation: String,
    pub(crate) acceleration_kind: Option<String>,
    pub(crate) acceleration_observer: Option<String>,
    pub(crate) acceleration_residency: Option<String>,
    pub(crate) acceleration_usage_site: Option<String>,
    pub(crate) validity: String,
    pub(crate) producer: String,
    pub(crate) consumer: String,
}

#[derive(Serialize)]
pub(crate) struct ObserverArtifactUseDump {
    pub(crate) actor: String,
    pub(crate) artifact_id: String,
    pub(crate) kind: String,
    pub(crate) source: String,
    pub(crate) required_validity: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct ObserverValidationSummaryDump {
    pub(crate) status: String,
    pub(crate) errors: Vec<String>,
}

#[derive(Serialize)]
pub(crate) struct PresentationBindingDump {
    pub(crate) id: String,
    pub(crate) pass_kind: String,
    pub(crate) recipe: String,
    pub(crate) default_backend: String,
    pub(crate) execution: String,
}

pub(crate) fn observer_semantic_artifact_dump(
    artifact: wrela::artifact_contract::SemanticArtifactContract,
) -> ObserverSemanticArtifactDump {
    let acceleration = artifact.acceleration.as_ref();
    ObserverSemanticArtifactDump {
        id: artifact.id.to_string(),
        kind: format!("{:?}", artifact.kind),
        logical_schema: artifact.logical_schema.describe().to_string(),
        snapshot_relation: wrela::artifact_contract::snapshot_relation_name(
            artifact.compatibility.snapshot,
        )
        .to_string(),
        acceleration_kind: acceleration.map(|value| {
            wrela::artifact_contract::acceleration_artifact_kind_name(value.kind).to_string()
        }),
        acceleration_observer: acceleration.map(|value| {
            wrela::artifact_contract::artifact_observer_name(value.observer).to_string()
        }),
        acceleration_residency: acceleration.map(|value| {
            wrela::artifact_contract::artifact_residency_name(value.residency).to_string()
        }),
        acceleration_usage_site: acceleration.map(|value| value.usage_site.to_string()),
        validity: format!("{:?}", artifact.validity),
        producer: artifact.producer.to_string(),
        consumer: artifact.consumer.to_string(),
    }
}

pub(crate) fn observer_artifact_use_dump(
    use_record: wrela::artifact_contract::ArtifactUse,
) -> ObserverArtifactUseDump {
    ObserverArtifactUseDump {
        actor: use_record.actor.to_string(),
        artifact_id: use_record.artifact_id.to_string(),
        kind: wrela::artifact_contract::artifact_use_kind_name(use_record.kind).to_string(),
        source: wrela::artifact_contract::artifact_use_source_name(use_record.source).to_string(),
        required_validity: use_record
            .required_validity
            .map(|validity| format!("{validity:?}")),
    }
}

pub(crate) fn observer_validation_summary(
    errors: impl IntoIterator<Item = String>,
) -> ObserverValidationSummaryDump {
    let errors = errors.into_iter().collect::<Vec<_>>();
    ObserverValidationSummaryDump {
        status: if errors.is_empty() {
            "ok".to_string()
        } else {
            "invalid".to_string()
        },
        errors,
    }
}

#[cfg(test)]
mod observer_report_tests {
    use super::observer_validation_summary;

    #[test]
    fn observer_validation_summary_marks_errors_invalid() {
        let summary = observer_validation_summary([String::from("missing artifact producer")]);
        assert_eq!(summary.status, "invalid");
        assert_eq!(
            summary.errors,
            vec![String::from("missing artifact producer")]
        );
    }
}

pub(crate) fn print_observer_projection_human(
    projection: &query_program_debug::ObserverProjectionDump,
) {
    println!(
        "  shared spine: observer={} owner={} inputs={} nodes={} dependencies={} outputs={} lossy_boundaries={}",
        projection.observer_kind,
        projection.execution_owner,
        projection.spine.inputs.len(),
        projection.spine.nodes.len(),
        projection.spine.dependencies.len(),
        projection.spine.outputs.len(),
        projection.lossy_boundaries.len()
    );
    println!(
        "  shared spine inputs: {}",
        format_spine_bindings(&projection.spine.inputs)
    );
    println!(
        "  shared spine primitive nodes: {}",
        format_spine_node_labels(&projection.spine.nodes, "primitive_invocation")
    );
    println!(
        "  shared spine artifacts: {}",
        format_spine_artifacts(&projection.spine.nodes)
    );
    println!(
        "  shared spine outputs: {}",
        format_spine_bindings(&projection.spine.outputs)
    );
    println!(
        "  shared spine observability: graph_structure={} artifact_lifecycle={} query_dependencies={} backend_dispatch={} output_bindings={} validation_summary={} runtime_trace_local_only={} observer_metrics_local_only={}",
        projection.spine.observability.graph_structure,
        projection.spine.observability.artifact_lifecycle,
        projection.spine.observability.query_dependencies,
        projection.spine.observability.backend_dispatch,
        projection.spine.observability.output_bindings,
        projection.spine.observability.validation_summary,
        projection.spine.observability.runtime_trace_local_only,
        projection.spine.observability.observer_metrics_local_only
    );
    println!(
        "  shared spine lossy boundaries: {}",
        format_spine_lossy_boundaries(&projection.lossy_boundaries)
    );
    println!(
        "  shared dependency graph: status={} roots={} leaves={} cycles={} artifact_edges={} policy_edges={} output_edges={}",
        projection.analysis.dependency.status,
        format_shared_nodes(&projection.analysis.dependency.root_nodes),
        format_shared_nodes(&projection.analysis.dependency.leaf_nodes),
        format_shared_nodes(&projection.analysis.dependency.cycle_nodes),
        projection.analysis.dependency.artifact_edge_count,
        projection.analysis.dependency.policy_edge_count,
        projection.analysis.dependency.output_edge_count,
    );
    println!(
        "  shared artifact lifetimes: status={} explicit={} store_backed={} preserved={}",
        projection.analysis.artifact_lifetimes.status,
        format_shared_nodes(&projection.analysis.artifact_lifetimes.explicit_artifacts),
        format_shared_store_backed_loads(
            &projection.analysis.artifact_lifetimes.store_backed_loads
        ),
        format_shared_nodes(&projection.analysis.artifact_lifetimes.preserved_artifacts),
    );
    print_shared_issues(
        "shared artifact lifetime issues",
        &projection.analysis.artifact_lifetimes.issues,
    );
    println!(
        "  shared policy summary: status={} requirements={}",
        projection.analysis.policy.status,
        format_shared_policy_requirements(&projection.analysis.policy.requirements),
    );
    print_shared_issues("shared policy issues", &projection.analysis.policy.issues);
    println!(
        "  shared backend summary: status={} active={} supported={} bindings={} dispatch_nodes={} backend_dispatch_enabled={}",
        projection.analysis.backend.status,
        format_shared_nodes(&projection.analysis.backend.active_backends),
        format_shared_nodes(&projection.analysis.backend.supported_backends),
        projection
            .analysis
            .backend
            .binding_count
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        format_shared_nodes(&projection.analysis.backend.dispatch_nodes),
        projection.analysis.backend.backend_dispatch_enabled,
    );
    println!(
        "  shared observability report: common={} local_only={} lossy={}",
        format_shared_nodes(&projection.analysis.observability.common_channels),
        format_shared_nodes(&projection.analysis.observability.local_only_channels),
        format_shared_observability_boundaries(&projection.analysis.observability.lossy_boundaries),
    );
    print_shared_issues(
        "shared dependency issues",
        &projection.analysis.dependency.issues,
    );
}

pub(crate) fn format_spine_bindings(bindings: &[query_program_debug::SpineBindingDump]) -> String {
    if bindings.is_empty() {
        "none".to_string()
    } else {
        bindings
            .iter()
            .map(|binding| format!("{}:{}({})", binding.binding, binding.schema, binding.role))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

pub(crate) fn format_spine_node_labels(
    nodes: &[query_program_debug::SpineNodeDump],
    family: &str,
) -> String {
    let labels = nodes
        .iter()
        .filter(|node| node.family == family)
        .map(|node| node.label.as_str())
        .collect::<Vec<_>>();
    if labels.is_empty() {
        "none".to_string()
    } else {
        labels.join(", ")
    }
}

pub(crate) fn format_spine_artifacts(nodes: &[query_program_debug::SpineNodeDump]) -> String {
    let artifacts = nodes
        .iter()
        .filter(|node| node.family == "artifact_store")
        .map(|node| {
            let artifact_id = node
                .artifact_ids
                .first()
                .map(String::as_str)
                .unwrap_or("none");
            format!("{}[{artifact_id}]", node.label)
        })
        .collect::<Vec<_>>();
    if artifacts.is_empty() {
        "none".to_string()
    } else {
        artifacts.join(", ")
    }
}

pub(crate) fn format_spine_lossy_boundaries(
    boundaries: &[query_program_debug::SpineLossyBoundaryDump],
) -> String {
    if boundaries.is_empty() {
        "none".to_string()
    } else {
        boundaries
            .iter()
            .map(|boundary| format!("{}({})", boundary.node_id, boundary.reason))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

pub(crate) fn format_shared_nodes(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
}

pub(crate) fn format_shared_store_backed_loads(
    loads: &[query_program_debug::SpineArtifactAccessSummaryDump],
) -> String {
    if loads.is_empty() {
        "none".to_string()
    } else {
        loads
            .iter()
            .map(|load| {
                format!(
                    "{}->{}({})",
                    load.actor,
                    load.artifact_id,
                    load.required_validity.as_deref().unwrap_or("none")
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

pub(crate) fn format_shared_policy_requirements(
    requirements: &[query_program_debug::SpinePolicyRequirementSummaryDump],
) -> String {
    if requirements.is_empty() {
        "none".to_string()
    } else {
        requirements
            .iter()
            .map(|requirement| {
                let backends = if requirement.backends.is_empty() {
                    "none".to_string()
                } else {
                    requirement.backends.join("|")
                };
                let supported = if requirement.supported_backends.is_empty() {
                    "none".to_string()
                } else {
                    requirement.supported_backends.join("|")
                };
                format!(
                    "{}[legal={} backends={} supported={} required_guarantee={} selected_method={}]",
                    requirement.label,
                    requirement.legal,
                    backends,
                    supported,
                    requirement.required_guarantee.as_deref().unwrap_or("none"),
                    requirement.selected_method.as_deref().unwrap_or("none"),
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

pub(crate) fn format_shared_observability_boundaries(
    boundaries: &[query_program_debug::SpineObservabilityBoundaryReportDump],
) -> String {
    if boundaries.is_empty() {
        "none".to_string()
    } else {
        boundaries
            .iter()
            .map(|boundary| {
                format!(
                    "{}({}:{})",
                    boundary.node_id, boundary.reason, boundary.dropped_field_count
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

pub(crate) fn print_shared_issues(
    label: &str,
    issues: &[query_program_debug::SharedSpineIssueDump],
) {
    if issues.is_empty() {
        return;
    }
    println!("  {}:", label);
    for issue in issues {
        println!("    {}: {}", issue.scope, issue.message);
    }
}
