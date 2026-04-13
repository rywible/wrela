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
                notes: binding.notes.into_iter().map(|note| note.to_string()).collect(),
            })
            .collect(),
        nodes: spine
            .nodes
            .into_iter()
            .map(|node| SpineNodeDump {
                id: node.id.to_string(),
                family: wrela::query_program_spine::spine_node_family_name(node.family)
                    .to_string(),
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
                required_validity: node.required_validity.map(|validity| format!("{validity:?}")),
                notes: node.notes.into_iter().map(|note| note.to_string()).collect(),
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
                required_validity: edge.required_validity.map(|validity| format!("{validity:?}")),
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
                notes: binding.notes.into_iter().map(|note| note.to_string()).collect(),
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
