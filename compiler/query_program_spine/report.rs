use crate::query_program_spine::validate::{
    SpineAnalysisStatus, SpineBackend, SpineDependencyAnalysis, SpinePolicyLegalitySummary,
    analyze_dependency_graph, spine_analysis_status_name, spine_backend_name,
    summarize_policy_legality, validate_artifact_lifetime,
};
use crate::query_program_spine::{
    ObserverKind, ObserverProjection, SpineObservabilitySummary, observer_kind_name,
    spine_lossy_reason_name,
};
use smol_str::SmolStr;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpineLocalOnlyFlags {
    pub runtime_trace_local_only: bool,
    pub observer_metrics_local_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpineObservabilityReport {
    pub summary: SpineObservabilitySummary,
    pub lossy_boundaries: Vec<SpineLossyBoundaryReport>,
    pub local_only: SpineLocalOnlyFlags,
    pub notes: Vec<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpineLossyBoundaryReport {
    pub node_id: SmolStr,
    pub reason: SmolStr,
    pub dropped_fields: Vec<SmolStr>,
}

pub type SpineObservabilityBoundaryReport = SpineLossyBoundaryReport;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpineBackendSummary {
    pub status: SpineAnalysisStatus,
    pub dispatch_observable: bool,
    pub explicit_backends: Vec<SpineBackend>,
    pub supported_backends: Vec<SpineBackend>,
    pub binding_count: Option<u32>,
    pub notes: Vec<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedQueryProgramSpineReport {
    pub observer_kind: ObserverKind,
    pub source_plan: SmolStr,
    pub execution_owner: SmolStr,
    pub observer_local_notes: Vec<SmolStr>,
    pub dependency: SpineDependencyAnalysis,
    pub artifact_lifetime: crate::query_program_spine::validate::SpineArtifactLifetimeValidation,
    pub policy: SpinePolicyLegalitySummary,
    pub backend: SpineBackendSummary,
    pub observability: SpineObservabilityReport,
    pub status: SpineAnalysisStatus,
}

pub type SharedSpineReport = SharedQueryProgramSpineReport;

pub fn build_shared_query_program_spine_report(
    projection: &ObserverProjection,
) -> SharedQueryProgramSpineReport {
    let dependency = analyze_dependency_graph(&projection.spine);
    let artifact_lifetime = validate_artifact_lifetime(&projection.spine);
    let policy = summarize_policy_legality(&projection.spine);
    let backend = summarize_backend(&projection.spine.observability, &policy);
    let observability = summarize_observability(projection);
    let status = aggregate_status([
        dependency.status,
        artifact_lifetime.status,
        policy.status,
        backend.status,
    ]);

    SharedQueryProgramSpineReport {
        observer_kind: projection.observer_kind,
        source_plan: projection.source_plan.clone(),
        execution_owner: projection.execution_owner.clone(),
        observer_local_notes: projection.observer_local_notes.clone(),
        dependency,
        artifact_lifetime,
        policy,
        backend,
        observability,
        status,
    }
}

pub fn shared_spine_report(projection: &ObserverProjection) -> SharedSpineReport {
    build_shared_query_program_spine_report(projection)
}

fn summarize_backend(
    observability: &SpineObservabilitySummary,
    policy: &SpinePolicyLegalitySummary,
) -> SpineBackendSummary {
    let mut explicit_backends = BTreeSet::<SpineBackend>::new();
    let mut supported_backends = BTreeSet::<SpineBackend>::new();
    let binding_count = policy
        .policy_nodes
        .iter()
        .filter_map(|node| node.binding_count)
        .reduce(|left, right| left + right);

    for node in &policy.policy_nodes {
        if let Some(backend) = node.backend.clone() {
            explicit_backends.insert(backend);
        }
        if let Some(backend) = node.backend_preference.clone() {
            supported_backends.insert(backend);
        }
        supported_backends.extend(node.supported_backends.iter().cloned());
    }

    let status = if policy.status == SpineAnalysisStatus::Invalid {
        SpineAnalysisStatus::Invalid
    } else if explicit_backends.is_empty() && supported_backends.is_empty() {
        SpineAnalysisStatus::Partial
    } else if observability.backend_dispatch {
        SpineAnalysisStatus::Valid
    } else {
        SpineAnalysisStatus::Partial
    };

    let mut notes = vec![SmolStr::new(format!(
        "backend_dispatch={}",
        observability.backend_dispatch
    ))];
    if !policy.illegal_combinations.is_empty() {
        notes.push(SmolStr::new(format!(
            "illegal_policy_combinations={}",
            policy.illegal_combinations.len()
        )));
    }

    SpineBackendSummary {
        status,
        dispatch_observable: observability.backend_dispatch,
        explicit_backends: explicit_backends.into_iter().collect(),
        supported_backends: supported_backends.into_iter().collect(),
        binding_count,
        notes,
    }
}

fn summarize_observability(projection: &ObserverProjection) -> SpineObservabilityReport {
    let summary = projection.spine.observability.clone();
    let lossy_boundaries = projection
        .lossy_boundaries
        .iter()
        .map(|boundary| SpineLossyBoundaryReport {
            node_id: boundary.node_id.clone(),
            reason: SmolStr::new(spine_lossy_reason_name(boundary.reason)),
            dropped_fields: boundary.dropped_fields.clone(),
        })
        .collect::<Vec<_>>();
    let local_only = SpineLocalOnlyFlags {
        runtime_trace_local_only: summary.runtime_trace_local_only,
        observer_metrics_local_only: summary.observer_metrics_local_only,
    };
    let notes = vec![
        SmolStr::new(format!(
            "observer_kind={}",
            observer_kind_name(projection.observer_kind)
        )),
        SmolStr::new(format!("lossy_boundaries={}", lossy_boundaries.len())),
        SmolStr::new(format!(
            "runtime_trace_local_only={}",
            summary.runtime_trace_local_only
        )),
        SmolStr::new(format!(
            "observer_metrics_local_only={}",
            summary.observer_metrics_local_only
        )),
    ];

    SpineObservabilityReport {
        summary,
        lossy_boundaries,
        local_only,
        notes,
    }
}

fn aggregate_status(
    statuses: impl IntoIterator<Item = SpineAnalysisStatus>,
) -> SpineAnalysisStatus {
    let mut saw_partial = false;
    for status in statuses {
        match status {
            SpineAnalysisStatus::Invalid => return SpineAnalysisStatus::Invalid,
            SpineAnalysisStatus::Partial => saw_partial = true,
            SpineAnalysisStatus::Valid => {}
        }
    }
    if saw_partial {
        SpineAnalysisStatus::Partial
    } else {
        SpineAnalysisStatus::Valid
    }
}

#[allow(dead_code)]
fn status_label(status: SpineAnalysisStatus) -> &'static str {
    spine_analysis_status_name(status)
}

#[allow(dead_code)]
fn backend_label(backend: &SpineBackend) -> SmolStr {
    spine_backend_name(backend)
}
