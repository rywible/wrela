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
