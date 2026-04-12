use crate::kernel::ir::{
    KernelBatchItemContract, KernelBatchQueryPlan, KernelBlock, KernelCaptureQueryPlan,
    KernelDispatchGrid, KernelExpr, KernelFunction, KernelModule, KernelPlanStage, KernelStmt,
    KernelWorldQueryPlan, ResolvedKernelDispatch,
};
use crate::portable::{
    BUILTIN_FIELD_PRIMITIVE_FUNCTIONS, BUILTIN_HELPER_FUNCTIONS, builtin_record_by_function,
};
use crate::query_contract::{
    CaptureKind, ParticipantContractKind, QueryCardinality, QueryContractId, QueryFamilyId,
    QuerySurfaceKind, QueryTargetKind, query_contract,
};
use crate::query_plan::{
    ArtifactContract, ArtifactSchema, BatchQueryKind, CaptureQueryKind, DerivedArtifact,
    DispatchRecordContract, QueryItemKind, QueryResultKind, ResultRecordContract, SceneDomainFlag,
    SemanticEvidenceOrigin, SemanticEvidenceScope, WorldQueryKind, batch_query_kind_for_descriptor,
    capture_query_kind_for_descriptor, world_query_kind_for_descriptor,
};
use crate::query_solver::is_ray_shaped_spatial_contract;
use crate::semantic_evidence::EvidenceRefinementKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelValidationError {
    pub message: String,
}

#[derive(Debug, Clone, Copy)]
struct QueryValidationContext {
    label: &'static str,
    contract_id: QueryContractId,
    contract_version: u32,
}

impl QueryValidationContext {
    fn message(self, detail: impl std::fmt::Display) -> String {
        format!(
            "{} contract '{}' v{}: {detail}",
            self.label,
            self.contract_id.as_str(),
            self.contract_version
        )
    }
}

pub fn validate_dispatch(
    dispatch: &ResolvedKernelDispatch,
) -> Result<(), Vec<KernelValidationError>> {
    let mut errors = Vec::new();
    validate_grid(dispatch.grid, &mut errors);
    if dispatch.kernel.is_empty() {
        errors.push(KernelValidationError {
            message: "dispatch kernel name must not be empty".to_string(),
        });
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub fn validate_module(module: &KernelModule) -> Result<(), Vec<KernelValidationError>> {
    let mut errors = Vec::new();
    if module.entry.is_empty() {
        errors.push(KernelValidationError {
            message: "kernel module entry must not be empty".to_string(),
        });
    } else if module.function(module.entry.as_str()).is_none() {
        errors.push(KernelValidationError {
            message: format!(
                "kernel module entry '{}' was not found in the module",
                module.entry.as_str()
            ),
        });
    }

    let mut seen = std::collections::BTreeSet::new();
    for function in &module.functions {
        if !seen.insert(function.name.clone()) {
            errors.push(KernelValidationError {
                message: format!("duplicate kernel function '{}'", function.name.as_str()),
            });
        }
        validate_function(module, function, &mut errors);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub fn validate_batch_query_plan(
    plan: &KernelBatchQueryPlan,
) -> Result<(), Vec<KernelValidationError>> {
    let mut errors = Vec::new();
    let context = QueryValidationContext {
        label: "batch query",
        contract_id: plan.contract_id,
        contract_version: plan.contract_version,
    };
    validate_plan_stages(context, &plan.stages, &mut errors);
    validate_contract_version(context, &mut errors);
    validate_artifact_contracts(
        context,
        &plan.artifact_contracts,
        Some(&plan.dispatch_contract),
        &plan.result_contract,
        &plan.derived_artifacts,
        &mut errors,
    );
    if !matches!(plan.stages.first(), Some(KernelPlanStage::SelectBackend)) {
        errors.push(KernelValidationError {
            message: format!(
                "batch query contract '{}' v{} helper '{}' must start with SelectBackend",
                plan.contract_id.as_str(),
                plan.contract_version,
                plan.helper_name
            ),
        });
    }
    validate_batch_item_contract(
        &plan.item_contract,
        &plan.result_contract,
        plan.contract_id,
        plan.contract_version,
        &mut errors,
    );
    if plan.candidate_contract.item_kind != plan.item_kind {
        errors.push(KernelValidationError {
            message: format!(
                "batch query contract '{}' v{} helper '{}' candidate item kind {:?} does not match plan item kind {:?}",
                plan.contract_id.as_str(),
                plan.contract_version,
                plan.helper_name,
                plan.candidate_contract.item_kind,
                plan.item_kind
            ),
        });
    }
    if plan.dispatch_contract.item_kind != plan.item_kind {
        errors.push(KernelValidationError {
            message: format!(
                "batch query contract '{}' v{} helper '{}' dispatch item kind {:?} does not match plan item kind {:?}",
                plan.contract_id.as_str(),
                plan.contract_version,
                plan.helper_name,
                plan.dispatch_contract.item_kind,
                plan.item_kind
            ),
        });
    }
    if plan.dispatch_contract.result_kind != plan.result_kind {
        errors.push(KernelValidationError {
            message: format!(
                "batch query contract '{}' v{} helper '{}' dispatch result kind {:?} does not match plan result kind {:?}",
                plan.contract_id.as_str(),
                plan.contract_version,
                plan.helper_name,
                plan.dispatch_contract.result_kind,
                plan.result_kind
            ),
        });
    }
    if plan.result_contract.result_kind != plan.result_kind {
        errors.push(KernelValidationError {
            message: format!(
                "batch query contract '{}' v{} helper '{}' result contract kind {:?} does not match plan result kind {:?}",
                plan.contract_id.as_str(),
                plan.contract_version,
                plan.helper_name,
                plan.result_contract.result_kind,
                plan.result_kind
            ),
        });
    }
    validate_query_contract_descriptor(
        "batch query",
        plan.contract_id,
        plan.family,
        plan.target,
        plan.cardinality,
        plan.surface,
        plan.item_kind,
        plan.result_kind,
        plan.contract_version,
        &plan.domain_flags,
        plan.participant_contract
            .as_ref()
            .map(|contract| contract.kind),
        plan.preserves_local_hit_context,
        &mut errors,
    );
    validate_batch_contract_authority(
        plan.contract_id,
        plan.contract_version,
        plan.kind,
        plan.capture_kind,
        &mut errors,
    );
    validate_ray_solver_presence(
        "batch query",
        plan.contract_id,
        plan.contract_version,
        plan.ray_solver.as_ref(),
        &mut errors,
    );
    if let Some(descriptor) = query_contract(plan.contract_id)
        && !descriptor.supported_backends.supports(plan.backend)
    {
        errors.push(KernelValidationError {
            message: format!(
                "batch query contract '{}' v{} does not support backend {:?}",
                plan.contract_id.as_str(),
                plan.contract_version,
                plan.backend
            ),
        });
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub fn validate_capture_query_plan(
    plan: &KernelCaptureQueryPlan,
) -> Result<(), Vec<KernelValidationError>> {
    let mut errors = Vec::new();
    let context = QueryValidationContext {
        label: "capture query",
        contract_id: plan.contract_id,
        contract_version: plan.contract_version,
    };
    validate_plan_stages(context, &plan.stages, &mut errors);
    validate_contract_version(context, &mut errors);
    validate_artifact_contracts(
        context,
        &plan.artifact_contracts,
        None,
        &plan.result_contract,
        &plan.derived_artifacts,
        &mut errors,
    );
    if plan.result_contract.result_kind != plan.result_kind {
        errors.push(KernelValidationError {
            message: format!(
                "capture query contract '{}' v{} helper '{}' result contract kind {:?} does not match plan result kind {:?}",
                plan.contract_id.as_str(),
                plan.contract_version,
                plan.helper_name,
                plan.result_contract.result_kind,
                plan.result_kind
            ),
        });
    }
    validate_query_contract_descriptor(
        "capture query",
        plan.contract_id,
        plan.family,
        plan.target,
        plan.cardinality,
        plan.surface,
        plan.candidate_contract.item_kind,
        plan.result_kind,
        plan.contract_version,
        &[],
        plan.participant_contract
            .as_ref()
            .map(|contract| contract.kind),
        plan.preserves_local_hit_context,
        &mut errors,
    );
    validate_capture_contract_authority(
        plan.contract_id,
        plan.contract_version,
        plan.kind,
        plan.capture_kind,
        &mut errors,
    );
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub fn validate_world_query_plan(
    plan: &KernelWorldQueryPlan,
) -> Result<(), Vec<KernelValidationError>> {
    let mut errors = Vec::new();
    let context = QueryValidationContext {
        label: "world query",
        contract_id: plan.contract_id,
        contract_version: plan.contract_version,
    };
    validate_plan_stages(context, &plan.stages, &mut errors);
    validate_contract_version(context, &mut errors);
    validate_artifact_contracts(
        context,
        &plan.artifact_contracts,
        Some(&plan.dispatch_contract),
        &plan.result_contract,
        &plan.derived_artifacts,
        &mut errors,
    );
    if plan.candidate_contract.item_kind != plan.dispatch_contract.item_kind {
        errors.push(KernelValidationError {
            message: format!(
                "world query contract '{}' v{} helper '{}' candidate item kind {:?} does not match dispatch item kind {:?}",
                plan.contract_id.as_str(),
                plan.contract_version,
                plan.helper_name,
                plan.candidate_contract.item_kind,
                plan.dispatch_contract.item_kind
            ),
        });
    }
    if plan.dispatch_contract.result_kind != plan.result_kind {
        errors.push(KernelValidationError {
            message: format!(
                "world query contract '{}' v{} helper '{}' dispatch result kind {:?} does not match plan result kind {:?}",
                plan.contract_id.as_str(),
                plan.contract_version,
                plan.helper_name,
                plan.dispatch_contract.result_kind,
                plan.result_kind
            ),
        });
    }
    if plan.result_contract.result_kind != plan.result_kind {
        errors.push(KernelValidationError {
            message: format!(
                "world query contract '{}' v{} helper '{}' result contract kind {:?} does not match plan result kind {:?}",
                plan.contract_id.as_str(),
                plan.contract_version,
                plan.helper_name,
                plan.result_contract.result_kind,
                plan.result_kind
            ),
        });
    }
    validate_query_contract_descriptor(
        "world query",
        plan.contract_id,
        plan.family,
        plan.target,
        plan.cardinality,
        plan.surface,
        plan.dispatch_contract.item_kind,
        plan.result_kind,
        plan.contract_version,
        &plan.domain_flags,
        plan.participant_contract
            .as_ref()
            .map(|contract| contract.kind),
        plan.preserves_local_hit_context,
        &mut errors,
    );
    validate_world_contract_authority(
        plan.contract_id,
        plan.contract_version,
        plan.kind,
        &mut errors,
    );
    validate_ray_solver_presence(
        "world query",
        plan.contract_id,
        plan.contract_version,
        plan.ray_solver.as_ref(),
        &mut errors,
    );
    if let Some(descriptor) = query_contract(plan.contract_id)
        && !descriptor.supported_backends.supports(plan.backend)
    {
        errors.push(KernelValidationError {
            message: format!(
                "world query contract '{}' v{} does not support backend {:?}",
                plan.contract_id.as_str(),
                plan.contract_version,
                plan.backend
            ),
        });
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn validate_ray_solver_presence(
    label: &str,
    contract_id: QueryContractId,
    contract_version: u32,
    solver: Option<&crate::query_solver::RaySolverPlan>,
    errors: &mut Vec<KernelValidationError>,
) {
    let ray_shaped = is_ray_shaped_spatial_contract(contract_id);
    match (ray_shaped, solver) {
        (true, None) => errors.push(KernelValidationError {
            message: format!(
                "{label} contract '{}' v{} must route through a RaySolverPlan",
                contract_id.as_str(),
                contract_version
            ),
        }),
        (false, Some(plan)) => errors.push(KernelValidationError {
            message: format!(
                "{label} contract '{}' v{} must not carry ray solver diagnostics from '{}'",
                contract_id.as_str(),
                contract_version,
                plan.id
            ),
        }),
        (true, Some(plan)) if plan.contract_id != contract_id => {
            errors.push(KernelValidationError {
                message: format!(
                    "{label} contract '{}' v{} has mismatched RaySolverPlan for '{}'",
                    contract_id.as_str(),
                    contract_version,
                    plan.contract_id.as_str()
                ),
            });
        }
        _ => {}
    }
}

fn validate_contract_version(
    context: QueryValidationContext,
    errors: &mut Vec<KernelValidationError>,
) {
    if context.contract_version == 0 {
        errors.push(KernelValidationError {
            message: context.message("contract version must be greater than zero"),
        });
    }
}

fn validate_query_contract_descriptor(
    label: &str,
    contract_id: QueryContractId,
    family: QueryFamilyId,
    target: QueryTargetKind,
    cardinality: QueryCardinality,
    surface: QuerySurfaceKind,
    item_kind: QueryItemKind,
    result_kind: QueryResultKind,
    contract_version: u32,
    domain_flags: &[SceneDomainFlag],
    participant_kind: Option<CaptureQueryKind>,
    preserves_local_hit_context: bool,
    errors: &mut Vec<KernelValidationError>,
) {
    let Some(descriptor) = query_contract(contract_id) else {
        errors.push(KernelValidationError {
            message: format!(
                "{label} contract '{}' v{} was not found in the query registry",
                contract_id.as_str(),
                contract_version
            ),
        });
        return;
    };

    if descriptor.version != contract_version {
        errors.push(KernelValidationError {
            message: format!(
                "{label} contract '{}' version {} does not match descriptor version {}",
                contract_id.as_str(),
                contract_version,
                descriptor.version
            ),
        });
    }
    if descriptor.family != family {
        errors.push(KernelValidationError {
            message: format!(
                "{label} contract '{}' v{} family {:?} does not match descriptor v{} family {:?}",
                contract_id.as_str(),
                contract_version,
                family,
                descriptor.version,
                descriptor.family
            ),
        });
    }
    if descriptor.target != target {
        errors.push(KernelValidationError {
            message: format!(
                "{label} contract '{}' v{} target {:?} does not match descriptor v{} target {:?}",
                contract_id.as_str(),
                contract_version,
                target,
                descriptor.version,
                descriptor.target
            ),
        });
    }
    if descriptor.cardinality != cardinality {
        errors.push(KernelValidationError {
            message: format!(
                "{label} contract '{}' v{} cardinality {:?} does not match descriptor v{} cardinality {:?}",
                contract_id.as_str(),
                contract_version,
                cardinality,
                descriptor.version,
                descriptor.cardinality
            ),
        });
    }
    if descriptor.surface != surface {
        errors.push(KernelValidationError {
            message: format!(
                "{label} contract '{}' v{} surface {:?} does not match descriptor v{} surface {:?}",
                contract_id.as_str(),
                contract_version,
                surface,
                descriptor.version,
                descriptor.surface
            ),
        });
    }
    if descriptor.item_kind != item_kind {
        errors.push(KernelValidationError {
            message: format!(
                "{label} contract '{}' v{} item kind {:?} does not match descriptor v{} item kind {:?}",
                contract_id.as_str(),
                contract_version,
                item_kind,
                descriptor.version,
                descriptor.item_kind
            ),
        });
    }
    if descriptor.result_kind != result_kind {
        errors.push(KernelValidationError {
            message: format!(
                "{label} contract '{}' v{} result kind {:?} does not match descriptor v{} result kind {:?}",
                contract_id.as_str(),
                contract_version,
                result_kind,
                descriptor.version,
                descriptor.result_kind
            ),
        });
    }
    if descriptor.required_domain_flags != domain_flags {
        errors.push(KernelValidationError {
            message: format!(
                "{label} contract '{}' v{} domain flags {:?} do not match descriptor v{} flags {:?}",
                contract_id.as_str(),
                contract_version,
                domain_flags,
                descriptor.version,
                descriptor.required_domain_flags
            ),
        });
    }
    let expected_participant_kind = descriptor.participant_kind.map(participant_query_kind);
    if expected_participant_kind != participant_kind {
        errors.push(KernelValidationError {
            message: format!(
                "{label} contract '{}' v{} participant selection {:?} does not match descriptor v{} {:?}",
                contract_id.as_str(),
                contract_version,
                participant_kind,
                descriptor.version,
                expected_participant_kind
            ),
        });
    }
    if descriptor.preserves_local_hit_context != preserves_local_hit_context {
        errors.push(KernelValidationError {
            message: format!(
                "{label} contract '{}' v{} local hit preservation {} does not match descriptor v{} {}",
                contract_id.as_str(),
                contract_version,
                preserves_local_hit_context,
                descriptor.version,
                descriptor.preserves_local_hit_context
            ),
        });
    }
}

fn validate_batch_contract_authority(
    contract_id: QueryContractId,
    contract_version: u32,
    kind: BatchQueryKind,
    capture_kind: CaptureKind,
    errors: &mut Vec<KernelValidationError>,
) {
    let Some(descriptor) = query_contract(contract_id) else {
        return;
    };
    if let Some(expected_kind) = batch_query_kind_for_descriptor(descriptor)
        && expected_kind != kind
    {
        errors.push(KernelValidationError {
            message: format!(
                "batch query contract '{}' v{} legacy kind {:?} does not match descriptor question {:?}",
                contract_id.as_str(),
                contract_version,
                kind,
                descriptor.question
            ),
        });
    }
    if descriptor.capture_kind != capture_kind {
        errors.push(KernelValidationError {
            message: format!(
                "batch query contract '{}' v{} capture kind {:?} does not match descriptor capture kind {:?}",
                contract_id.as_str(),
                contract_version,
                capture_kind,
                descriptor.capture_kind
            ),
        });
    }
}

fn validate_capture_contract_authority(
    contract_id: QueryContractId,
    contract_version: u32,
    kind: CaptureQueryKind,
    capture_kind: CaptureKind,
    errors: &mut Vec<KernelValidationError>,
) {
    let Some(descriptor) = query_contract(contract_id) else {
        return;
    };
    if let Some(expected_kind) = capture_query_kind_for_descriptor(descriptor)
        && expected_kind != kind
    {
        errors.push(KernelValidationError {
            message: format!(
                "capture query contract '{}' v{} legacy kind {:?} does not match descriptor question {:?}",
                contract_id.as_str(),
                contract_version,
                kind,
                descriptor.question
            ),
        });
    }
    if descriptor.capture_kind != capture_kind {
        errors.push(KernelValidationError {
            message: format!(
                "capture query contract '{}' v{} capture kind {:?} does not match descriptor capture kind {:?}",
                contract_id.as_str(),
                contract_version,
                capture_kind,
                descriptor.capture_kind
            ),
        });
    }
}

fn validate_world_contract_authority(
    contract_id: QueryContractId,
    contract_version: u32,
    kind: WorldQueryKind,
    errors: &mut Vec<KernelValidationError>,
) {
    let Some(descriptor) = query_contract(contract_id) else {
        return;
    };
    if let Some(expected_kind) = world_query_kind_for_descriptor(descriptor)
        && expected_kind != kind
    {
        errors.push(KernelValidationError {
            message: format!(
                "world query contract '{}' v{} legacy kind {:?} does not match descriptor question {:?}",
                contract_id.as_str(),
                contract_version,
                kind,
                descriptor.question
            ),
        });
    }
}

fn participant_query_kind(kind: ParticipantContractKind) -> CaptureQueryKind {
    match kind {
        ParticipantContractKind::Radiance => CaptureQueryKind::Radiance,
        ParticipantContractKind::Medium => CaptureQueryKind::Medium,
    }
}

fn validate_batch_item_contract(
    contract: &KernelBatchItemContract,
    result: &crate::query_plan::ResultRecordContract,
    batch_contract_id: QueryContractId,
    batch_contract_version: u32,
    errors: &mut Vec<KernelValidationError>,
) {
    match contract {
        KernelBatchItemContract::CaptureQuery { plan } => {
            if let Err(plan_errors) = validate_capture_query_plan(plan) {
                errors.extend(plan_errors);
            }
            if plan.result_kind != result.result_kind {
                errors.push(KernelValidationError {
                    message: format!(
                        "batch query contract '{}' v{} capture item contract result kind {:?} does not match batch result contract {:?}",
                        batch_contract_id.as_str(),
                        batch_contract_version,
                        plan.result_kind,
                        result.result_kind
                    ),
                });
            }
            if plan.contract_version != batch_contract_version {
                errors.push(KernelValidationError {
                    message: format!(
                        "batch query contract '{}' v{} capture item contract version {} does not match the parent batch contract",
                        batch_contract_id.as_str(),
                        batch_contract_version,
                        plan.contract_version
                    ),
                });
            }
        }
        KernelBatchItemContract::RayThenOcclusion { nearest_plan } => {
            if let Err(plan_errors) = validate_capture_query_plan(nearest_plan) {
                errors.extend(plan_errors);
            }
            if !matches!(
                result.result_kind,
                crate::query_plan::QueryResultKind::OcclusionResult
            ) {
                errors.push(KernelValidationError {
                    message: format!(
                        "batch query contract '{}' v{} RayThenOcclusion contracts must produce OcclusionResult",
                        batch_contract_id.as_str(),
                        batch_contract_version
                    ),
                });
            }
            if !matches!(
                nearest_plan.result_kind,
                crate::query_plan::QueryResultKind::Hit3
            ) {
                errors.push(KernelValidationError {
                    message: format!(
                        "batch query contract '{}' v{} RayThenOcclusion contracts must embed a Hit3 nearest plan",
                        batch_contract_id.as_str(),
                        batch_contract_version
                    ),
                });
            }
            if nearest_plan.contract_version != batch_contract_version {
                errors.push(KernelValidationError {
                    message: format!(
                        "batch query contract '{}' v{} RayThenOcclusion nearest plan version {} does not match the parent batch contract",
                        batch_contract_id.as_str(),
                        batch_contract_version,
                        nearest_plan.contract_version
                    ),
                });
            }
        }
        KernelBatchItemContract::WorldQuery { plan } => {
            if let Err(plan_errors) = validate_world_query_plan(plan) {
                errors.extend(plan_errors);
            }
            if plan.result_kind != result.result_kind {
                errors.push(KernelValidationError {
                    message: format!(
                        "batch query contract '{}' v{} world item contract result kind {:?} does not match batch result contract {:?}",
                        batch_contract_id.as_str(),
                        batch_contract_version,
                        plan.result_kind,
                        result.result_kind
                    ),
                });
            }
            if plan.contract_version != batch_contract_version {
                errors.push(KernelValidationError {
                    message: format!(
                        "batch query contract '{}' v{} world item contract version {} does not match the parent batch contract",
                        batch_contract_id.as_str(),
                        batch_contract_version,
                        plan.contract_version
                    ),
                });
            }
        }
    }
}

fn validate_artifact_contracts(
    context: QueryValidationContext,
    artifacts: &[ArtifactContract],
    dispatch: Option<&DispatchRecordContract>,
    result: &ResultRecordContract,
    derived: &[DerivedArtifact],
    errors: &mut Vec<KernelValidationError>,
) {
    for artifact in artifacts {
        if artifact.id.is_empty() {
            errors.push(KernelValidationError {
                message: context.message("artifact contracts must carry a stable id"),
            });
        }
        if artifact.version == 0 {
            errors.push(KernelValidationError {
                message: context.message(format!(
                    "artifact contract '{}' must have a non-zero version",
                    artifact.id
                )),
            });
        }
        if !matches!(
            artifact.evidence_summary.origin,
            SemanticEvidenceOrigin::ArtifactDerived
        ) {
            errors.push(KernelValidationError {
                message: context.message(format!(
                    "artifact contract '{}' must report artifact-derived evidence origin",
                    artifact.id
                )),
            });
        }
        if !matches!(
            artifact.evidence_summary.scope,
            SemanticEvidenceScope::ArtifactBound
        ) {
            errors.push(KernelValidationError {
                message: context.message(format!(
                    "artifact contract '{}' must report artifact-bound evidence scope",
                    artifact.id
                )),
            });
        }
        if !artifact
            .evidence_summary
            .refinement_path
            .iter()
            .any(|step| matches!(step.kind, EvidenceRefinementKind::ArtifactBinding))
        {
            errors.push(KernelValidationError {
                message: context.message(format!(
                    "artifact contract '{}' must preserve scene-derived artifact refinement path",
                    artifact.id
                )),
            });
        }
    }

    if let Some(dispatch) = dispatch {
        let has_dispatch_contract = artifacts.iter().any(|artifact| {
            matches!(
                artifact.schema,
                ArtifactSchema::DispatchRecord {
                    item_kind,
                    result_kind,
                } if item_kind == dispatch.item_kind && result_kind == dispatch.result_kind
            )
        });
        if !has_dispatch_contract {
            errors.push(KernelValidationError {
                message: context.message(
                    "dispatch artifact contract does not match the dispatch record contract",
                ),
            });
        }
    }

    let has_result_contract = artifacts.iter().any(|artifact| {
        matches!(
            artifact.schema,
            ArtifactSchema::HitResultBuffer {
                result_kind,
                preserves_local_hit_context,
            } if result_kind == result.result_kind
                && preserves_local_hit_context == result.preserves_local_hit_context
        )
    });
    if !has_result_contract {
        errors.push(KernelValidationError {
            message: context
                .message("result artifact contract does not match the result record contract"),
        });
    }

    for artifact in derived {
        let matches_schema = artifacts
            .iter()
            .any(|contract| match (&contract.schema, artifact) {
                (
                    ArtifactSchema::SupportSummary {
                        semantics,
                        support_class,
                        can_coarse_support_pruning,
                        semantic_root,
                        support_root,
                        node_count,
                        support_node_count,
                        leaf_count,
                        identity_source_count,
                    },
                    DerivedArtifact::SupportSummary {
                        semantics: expected_semantics,
                        support_class: expected_support_class,
                        can_coarse_support_pruning: expected_pruning,
                    },
                ) => {
                    semantics == expected_semantics
                        && support_class == expected_support_class
                        && can_coarse_support_pruning == expected_pruning
                        && (*semantic_root == 0 || *node_count != 0)
                        && (*support_root == 0 || *support_node_count != 0)
                        && *leaf_count <= *node_count
                        && *identity_source_count <= *node_count
                }
                (
                    ArtifactSchema::CaptureCache {
                        capture_kind,
                        semantic_root: _,
                    },
                    DerivedArtifact::CaptureCache {
                        capture_kind: expected_capture_kind,
                    },
                ) => capture_kind == expected_capture_kind,
                (
                    ArtifactSchema::CullingTable {
                        candidate_strategy,
                        pruning_strategy,
                        support_root,
                        support_node_count,
                        leaf_count,
                        identity_source_count,
                        ..
                    },
                    DerivedArtifact::CullingTable {
                        candidate_strategy: expected_candidate_strategy,
                        pruning_strategy: expected_pruning_strategy,
                    },
                ) => {
                    candidate_strategy == expected_candidate_strategy
                        && pruning_strategy == expected_pruning_strategy
                        && (*support_root == 0 || *support_node_count != 0)
                        && *leaf_count <= *support_node_count
                        && *identity_source_count <= *support_node_count
                }
                (
                    ArtifactSchema::OpaquePessimizationBoundary {
                        support_root,
                        support_node_count,
                    },
                    DerivedArtifact::OpaquePessimizationBoundary,
                ) => *support_root == 0 || *support_node_count != 0,
                _ => false,
            });
        if !matches_schema {
            errors.push(KernelValidationError {
                message: context.message(format!(
                    "missing artifact contract for derived artifact '{artifact:?}'"
                )),
            });
        }
    }
}

fn validate_grid(grid: KernelDispatchGrid, errors: &mut Vec<KernelValidationError>) {
    if grid.total_count().is_none() {
        errors.push(KernelValidationError {
            message: "dispatch grid overflowed the host-side reference interpreter".to_string(),
        });
    }
}

fn validate_plan_stages(
    context: QueryValidationContext,
    stages: &[KernelPlanStage],
    errors: &mut Vec<KernelValidationError>,
) {
    if stages.is_empty() {
        errors.push(KernelValidationError {
            message: context.message("kernel plan must contain at least one stage"),
        });
        return;
    }

    let begin = stages
        .iter()
        .position(|stage| matches!(stage, KernelPlanStage::BeginVirtualGpuDispatch));
    let end = stages
        .iter()
        .position(|stage| matches!(stage, KernelPlanStage::EndVirtualGpuDispatch));
    if begin.is_some() != end.is_some() {
        errors.push(KernelValidationError {
            message: context
                .message("virtual GPU dispatch scaffolding must include both begin and end stages"),
        });
    }

    let execute = stages
        .iter()
        .position(|stage| matches!(stage, KernelPlanStage::Execute { .. }));
    let append = stages
        .iter()
        .position(|stage| matches!(stage, KernelPlanStage::AppendResult { .. }));
    let iterate = stages
        .iter()
        .position(|stage| matches!(stage, KernelPlanStage::IterateItems { .. }));
    let load_capture = stages
        .iter()
        .position(|stage| matches!(stage, KernelPlanStage::LoadCapture));

    if let Some(iterate) = iterate {
        if let Some(load_capture) = load_capture
            && iterate < load_capture
        {
            errors.push(KernelValidationError {
                message: context.message("IterateItems must happen after LoadCapture"),
            });
        }
    }
    if let (Some(iterate), Some(execute)) = (iterate, execute)
        && execute < iterate
    {
        errors.push(KernelValidationError {
            message: context.message("Execute must happen after IterateItems"),
        });
    }
    if let (Some(execute), Some(append)) = (execute, append)
        && append < execute
    {
        errors.push(KernelValidationError {
            message: context.message("AppendResult must happen after Execute"),
        });
    }
    if let (Some(begin), Some(end)) = (begin, end)
        && end < begin
    {
        errors.push(KernelValidationError {
            message: context
                .message("EndVirtualGpuDispatch must happen after BeginVirtualGpuDispatch"),
        });
    }
}

fn validate_function(
    module: &KernelModule,
    function: &KernelFunction,
    errors: &mut Vec<KernelValidationError>,
) {
    validate_block(module, &function.body, errors, 0, function.name.as_str());
}

fn validate_block(
    module: &KernelModule,
    block: &KernelBlock,
    errors: &mut Vec<KernelValidationError>,
    loop_depth: usize,
    function_name: &str,
) {
    for stmt in block {
        match stmt {
            KernelStmt::Let { value, .. }
            | KernelStmt::Assign { value, .. }
            | KernelStmt::Expr { value, .. }
            | KernelStmt::IgnoreResult { value, .. } => {
                validate_expr(module, value, errors, function_name);
            }
            KernelStmt::If {
                condition,
                then_block,
                else_block,
                ..
            } => {
                validate_expr(module, condition, errors, function_name);
                validate_block(module, then_block, errors, loop_depth, function_name);
                validate_block(module, else_block, errors, loop_depth, function_name);
            }
            KernelStmt::While {
                condition, body, ..
            } => {
                validate_expr(module, condition, errors, function_name);
                validate_block(module, body, errors, loop_depth + 1, function_name);
            }
            KernelStmt::Return { value, .. } => {
                if let Some(value) = value {
                    validate_expr(module, value, errors, function_name);
                }
            }
            KernelStmt::Break { .. } | KernelStmt::Continue { .. } if loop_depth == 0 => {
                errors.push(KernelValidationError {
                    message: format!(
                        "kernel function '{}' uses loop control outside of a loop",
                        function_name
                    ),
                });
            }
            KernelStmt::Break { .. } | KernelStmt::Continue { .. } => {}
        }
    }
}

fn validate_expr(
    module: &KernelModule,
    expr: &KernelExpr,
    errors: &mut Vec<KernelValidationError>,
    function_name: &str,
) {
    match expr {
        KernelExpr::Unary { expr, .. } | KernelExpr::Crash { expr, .. } => {
            validate_expr(module, expr, errors, function_name);
        }
        KernelExpr::Binary { lhs, rhs, .. } => {
            validate_expr(module, lhs, errors, function_name);
            validate_expr(module, rhs, errors, function_name);
        }
        KernelExpr::Call { target, args, .. } => {
            for arg in args {
                validate_expr(module, arg, errors, function_name);
            }
            if module.function(target.as_str()).is_none() && !looks_like_builtin(target.as_str()) {
                errors.push(KernelValidationError {
                    message: format!(
                        "kernel function '{}' calls unknown target '{}'",
                        function_name,
                        target.as_str()
                    ),
                });
            }
        }
        KernelExpr::CaptureQuery { plan, args, .. } => {
            for arg in args {
                validate_expr(module, arg, errors, function_name);
            }
            if let Err(plan_errors) = validate_capture_query_plan(plan) {
                errors.extend(plan_errors);
            }
        }
        KernelExpr::WorldQuery { plan, args, .. } => {
            for arg in args {
                validate_expr(module, arg, errors, function_name);
            }
            if let Err(plan_errors) = validate_world_query_plan(plan) {
                errors.extend(plan_errors);
            }
        }
        KernelExpr::BatchQuery { plan, args, .. } => {
            for arg in args {
                validate_expr(module, arg, errors, function_name);
            }
            if let Err(plan_errors) = validate_batch_query_plan(plan) {
                errors.extend(plan_errors);
            }
        }
        KernelExpr::Member { base, .. } => validate_expr(module, base, errors, function_name),
        KernelExpr::Index { base, index, .. } => {
            validate_expr(module, base, errors, function_name);
            validate_expr(module, index, errors, function_name);
        }
        KernelExpr::ArrayLiteral { items, .. } => {
            for item in items {
                validate_expr(module, item, errors, function_name);
            }
        }
        KernelExpr::StructLiteral { fields, .. } => {
            for (_, expr) in fields {
                validate_expr(module, expr, errors, function_name);
            }
        }
        KernelExpr::Literal { .. }
        | KernelExpr::Var { .. }
        | KernelExpr::Capture { .. }
        | KernelExpr::DispatchBackend { .. } => {}
    }
}

fn looks_like_builtin(name: &str) -> bool {
    builtin_record_by_function(name).is_some()
        || BUILTIN_HELPER_FUNCTIONS.contains(&name)
        || BUILTIN_FIELD_PRIMITIVE_FUNCTIONS.contains(&name)
        || matches!(
            name,
            "i32"
                | "u32"
                | "f32"
                | "vec2"
                | "vec3"
                | "vec4"
                | "quat"
                | "mat3_identity"
                | "mat3_cols"
                | "mat4_identity"
                | "mat4_cols"
                | "dot"
                | "cross"
                | "min"
                | "max"
                | "clamp"
                | "mix"
                | "abs"
                | "sign"
                | "floor"
                | "ceil"
                | "fract"
                | "sin"
                | "cos"
                | "sqrt"
                | "pow"
                | "length"
                | "normalize"
                | "distance"
                | "reflect"
                | "translate"
                | "rotate"
                | "uniform_scale"
                | "affine_transform"
                | "warp"
                | "repeat_linear"
                | "repeat_grid"
                | "radial_repeat"
                | "mirror_array"
                | "instance_array"
                | "global_invocation_id"
                | "local_invocation_id"
                | "workgroup_id"
                | "num_workgroups"
                | "workgroup_size"
                | "workgroup_barrier"
                | "storage_barrier"
                | "gpu_buffer_new"
                | "gpu_buffer_len"
                | "gpu_buffer_get"
                | "gpu_buffer_set"
                | "gpu_atomic_i32_new"
                | "gpu_atomic_i32_drop"
                | "gpu_atomic_i32_load"
                | "gpu_atomic_i32_store"
                | "gpu_atomic_i32_fetch_add"
                | "gpu_atomic_u32_new"
                | "gpu_atomic_u32_drop"
                | "gpu_atomic_u32_load"
                | "gpu_atomic_u32_store"
                | "gpu_atomic_u32_fetch_add"
        )
}
