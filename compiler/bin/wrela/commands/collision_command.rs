//! Owns CLI-facing collision command orchestration and the collision report
//! projections consumed by human and JSON surfaces.
//! Does not own CLI token parsing or the collision runtime implementation.
//!
//! Key invariants:
//! - command handlers consume typed parse results; they must not recover command
//!   legality from raw argv fragments.
//! - collision plan/run reports describe the backend and witness semantics that
//!   actually executed.
//! - catalog, plan, and run dumps are projections of the same collision
//!   contract/runtime model.
//!
//! Primary entrypoints:
//! - `execute_collision_contracts_command`
//! - `execute_collision_plan_command`
//! - `execute_collision_run_command`
//!
//! Failure modes / common pitfalls:
//! - letting report helpers invent backend labels instead of using runtime state
//!   makes closure evidence untrustworthy.
//! - moving parse-time validation back into this module would regress the typed
//!   CLI contract Phase 54 is closing.

use super::observer_projection::*;
use super::presentation_reports::*;
use super::*;

pub(crate) fn execute_collision_contracts_command(args: CatalogCommandArgs) {
    let catalog = collision_contract_catalog_snapshot();
    if matches!(args.output_format, OutputFormat::Json) {
        println!(
            "{}",
            serde_json::to_string_pretty(&catalog).unwrap_or_else(|_| "{}".to_string())
        );
        return;
    }
    println!(
        "collision contract catalog schema v{}",
        catalog.schema_version
    );
    for contract in &catalog.contracts {
        let backends = if contract.backends.is_empty() {
            "none".to_string()
        } else {
            contract.backends.join(",")
        };
        println!(
            "{} v{} family={} question={} target={} authority=scope={} requires_previous_snapshot={} evidence_scope={} transition_compatibility={} input={}({}) output={}({}) witness={} backends={} policy=backend_preference={} required_guarantee={} selected_method={}",
            contract.contract_id,
            contract.contract_version,
            contract.family,
            contract.question,
            contract.target,
            contract.authority.scope,
            contract.authority.requires_previous_snapshot,
            contract.authority.required_evidence_scope,
            contract
                .authority
                .transition_compatibility
                .as_deref()
                .unwrap_or("none"),
            contract.input_kind,
            contract.input_record,
            contract.output_kind,
            contract.output_record,
            contract.witness_schema.name,
            backends,
            contract.policy.backend_preference,
            contract.policy.required_guarantee,
            contract.policy.selected_method,
        );
    }
}

pub(crate) fn execute_collision_plan_command(args: CollisionCommandArgs) {
    let dump = collision_plan_dump(args.query_backend);
    if matches!(args.output_format, OutputFormat::Json) {
        println!(
            "{}",
            serde_json::to_string_pretty(&dump).unwrap_or_else(|_| "{}".to_string())
        );
        return;
    }
    print_collision_plan_human(&dump);
}

pub(crate) fn execute_collision_run_command(args: CollisionCommandArgs) {
    let report = match collision_run_report(args.query_backend) {
        Ok(report) => report,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(EXIT_CODEGEN);
        }
    };
    if matches!(args.output_format, OutputFormat::Json) {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string())
        );
    } else {
        print_collision_run_human(&report);
    }
}

#[derive(Serialize)]
pub(crate) struct CollisionContractCatalogDump {
    pub(crate) schema_version: u32,
    pub(crate) contracts: Vec<CollisionContractCatalogItemDump>,
}

#[derive(Serialize)]
pub(crate) struct CollisionContractCatalogItemDump {
    pub(crate) contract_id: String,
    pub(crate) contract_version: u32,
    pub(crate) family: String,
    pub(crate) question: String,
    pub(crate) target: String,
    pub(crate) authority: CollisionAuthorityRequirementDump,
    pub(crate) input_kind: String,
    pub(crate) input_record: String,
    pub(crate) output_kind: String,
    pub(crate) output_record: String,
    pub(crate) witness_schema: CollisionWitnessSchemaDump,
    pub(crate) policy: CollisionExecutionPolicyDump,
    pub(crate) backends: Vec<String>,
}

#[derive(Serialize)]
pub(crate) struct CollisionWitnessSchemaDump {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) fields: Vec<CollisionWitnessFieldDump>,
}

#[derive(Serialize)]
pub(crate) struct CollisionWitnessFieldDump {
    pub(crate) name: String,
    pub(crate) ty: String,
}

#[derive(Serialize)]
pub(crate) struct CollisionExecutionPolicyDump {
    pub(crate) backend_preference: String,
    pub(crate) required_guarantee: String,
    pub(crate) selected_method: String,
}

#[derive(Serialize)]
pub(crate) struct CollisionAuthorityRequirementDump {
    pub(crate) scope: String,
    pub(crate) requires_previous_snapshot: bool,
    pub(crate) required_evidence_scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) transition_compatibility: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct CollisionPlanCatalogDump {
    pub(crate) schema_version: u32,
    pub(crate) backend: String,
    pub(crate) plans: Vec<CollisionPlanDumpItem>,
}

#[derive(Serialize)]
pub(crate) struct CollisionRunReport {
    pub(crate) schema_version: u32,
    pub(crate) backend: String,
    pub(crate) executions: Vec<CollisionExecutionDump>,
}

#[derive(Serialize)]
pub(crate) struct CollisionExecutionDump {
    pub(crate) name: String,
    pub(crate) plan_name: String,
    pub(crate) contract_id: String,
    pub(crate) target: String,
    pub(crate) authority_scope: String,
    pub(crate) runtime_ns: u128,
    pub(crate) result: CollisionResultDump,
    pub(crate) trace: CollisionExecutionTraceDump,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum CollisionResultDump {
    Occupancy {
        occupied: bool,
        classification: String,
        signed_distance: f32,
        witness: CollisionPointWitnessDump,
    },
    RayCast {
        hit: bool,
        miss_reason: String,
        witness: Option<CollisionRayWitnessDump>,
    },
    SphereOverlap {
        overlaps: bool,
        signed_separation: f32,
        witness: CollisionSphereWitnessDump,
    },
    Sweep {
        hit: bool,
        witness: Option<CollisionSweepWitnessDump>,
        no_hit_certificate: Option<CollisionNoHitCertificateDump>,
    },
    TimeOfImpact {
        hit: bool,
        time_fraction_upper_bound: Option<f32>,
        witness: Option<CollisionTimeOfImpactWitnessDump>,
        no_hit_certificate: Option<CollisionNoHitCertificateDump>,
    },
}

#[derive(Serialize)]
pub(crate) struct CollisionPointWitnessDump {
    pub(crate) sample_point: [f32; 3],
    pub(crate) nearest_point_on_world: [f32; 3],
    pub(crate) world_normal: [f32; 3],
    pub(crate) signed_distance: f32,
    pub(crate) normal_provenance: String,
}

#[derive(Serialize)]
pub(crate) struct CollisionRayWitnessDump {
    pub(crate) travel_distance: f32,
    pub(crate) position: [f32; 3],
    pub(crate) normal: [f32; 3],
    pub(crate) root_shape_id: u32,
    pub(crate) feature_id: u32,
    pub(crate) normal_provenance: String,
}

#[derive(Serialize)]
pub(crate) struct CollisionSphereWitnessDump {
    pub(crate) point_on_probe: [f32; 3],
    pub(crate) point_on_world: [f32; 3],
    pub(crate) world_normal: [f32; 3],
    pub(crate) signed_separation: f32,
    pub(crate) normal_provenance: String,
}

#[derive(Serialize)]
pub(crate) struct CollisionSweepWitnessDump {
    pub(crate) contact_fraction_upper_bound: f32,
    pub(crate) point_on_probe: [f32; 3],
    pub(crate) point_on_world: [f32; 3],
    pub(crate) contact_normal: [f32; 3],
    pub(crate) normal_flavor: String,
    pub(crate) normal_provenance: String,
}

#[derive(Serialize)]
pub(crate) struct CollisionTimeOfImpactWitnessDump {
    pub(crate) time_fraction_upper_bound: f32,
    pub(crate) point_on_probe: [f32; 3],
    pub(crate) point_on_world: [f32; 3],
    pub(crate) contact_normal: [f32; 3],
    pub(crate) normal_flavor: String,
    pub(crate) normal_provenance: String,
}

#[derive(Serialize)]
pub(crate) struct CollisionNoHitCertificateDump {
    pub(crate) valid_through_fraction: f32,
    pub(crate) guarantee: String,
}

#[derive(Serialize)]
pub(crate) struct CollisionExecutionTraceDump {
    pub(crate) contract_id: String,
    pub(crate) family: String,
    pub(crate) question: String,
    pub(crate) backend: String,
    pub(crate) snapshot: Option<wrela::world_identity::SnapshotIdentityReport>,
    pub(crate) transition: Option<CollisionTransitionDump>,
    pub(crate) required_guarantee: String,
    pub(crate) selected_method: String,
    pub(crate) executed_query_contracts: Vec<String>,
    pub(crate) broadphase_candidate_count: u32,
    pub(crate) broadphase_rejected_candidate_count: u32,
    pub(crate) broadphase_pruned_node_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) interval_bracket: Option<[f32; 2]>,
    pub(crate) interval_subdivisions: u32,
    pub(crate) interval_refinements: u32,
    pub(crate) certificate_successes: u32,
    pub(crate) fallback_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) contact_normal_provenance: Option<String>,
    pub(crate) reuse_metrics: CollisionReuseMetricsDump,
    pub(crate) reuse_decisions: Vec<CollisionReuseDecisionDump>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) wgsl_metrics: Option<CollisionWgslMetricsDump>,
}

#[derive(Serialize)]
pub(crate) struct CollisionTransitionDump {
    pub(crate) current_snapshot_epoch: u32,
    pub(crate) previous_snapshot_epoch: u32,
    pub(crate) change_class: String,
}

#[derive(Serialize)]
pub(crate) struct CollisionReuseMetricsDump {
    pub(crate) available_count: u32,
    pub(crate) consumed_count: u32,
    pub(crate) rejected_count: u32,
    pub(crate) unavailable_count: u32,
    pub(crate) diagnostics: Vec<String>,
}

#[derive(Serialize)]
pub(crate) struct CollisionReuseDecisionDump {
    pub(crate) artifact_id: String,
    pub(crate) kind: String,
    pub(crate) verdict: String,
    pub(crate) reason: String,
    pub(crate) detail: String,
}

#[derive(Serialize)]
pub(crate) struct CollisionWgslMetricsDump {
    pub(crate) dispatch_count: u32,
    pub(crate) dispatch_items: u32,
    pub(crate) candidate_reduction_effectiveness: f32,
    pub(crate) selected_workgroup_size: u32,
    pub(crate) resident_shared_snapshot_artifacts: u32,
    pub(crate) cpu_certification_query_count: u32,
}

#[derive(Serialize)]
pub(crate) struct CollisionPlanDumpItem {
    pub(crate) name: String,
    pub(crate) contract_id: String,
    pub(crate) contract_version: u32,
    pub(crate) family: String,
    pub(crate) question: String,
    pub(crate) target: String,
    pub(crate) authority_scope: String,
    pub(crate) backend: String,
    pub(crate) policy: CollisionExecutionPolicyDump,
    pub(crate) inputs: Vec<CollisionPlanInputDump>,
    pub(crate) passes: Vec<CollisionPlanPassDump>,
    pub(crate) artifacts: Vec<CollisionArtifactBindingDump>,
    pub(crate) artifact_uses: Vec<ObserverArtifactUseDump>,
    pub(crate) outputs: Vec<CollisionPlanOutputDump>,
    pub(crate) observer_projection: query_program_debug::ObserverProjectionDump,
    pub(crate) validation: ObserverValidationSummaryDump,
}

#[derive(Serialize)]
pub(crate) struct CollisionArtifactBindingDump {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) record: String,
    pub(crate) contract: ObserverSemanticArtifactDump,
}

#[derive(Serialize)]
pub(crate) struct CollisionPlanInputDump {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) record: String,
}

#[derive(Serialize)]
pub(crate) struct CollisionPlanOutputDump {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) record: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) witness_schema: Option<CollisionWitnessSchemaDump>,
}

#[derive(Serialize)]
pub(crate) struct CollisionPlanPassDump {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) consumes: Vec<String>,
    pub(crate) materializes: Vec<String>,
    pub(crate) query_dependencies: Vec<String>,
}

pub(crate) fn collision_contract_catalog_snapshot() -> CollisionContractCatalogDump {
    let contracts = wrela::collision_contract::collision_contracts()
        .iter()
        .map(collision_contract_dump)
        .collect();
    CollisionContractCatalogDump {
        schema_version: wrela::collision_contract::COLLISION_CONTRACT_VERSION,
        contracts,
    }
}

pub(crate) fn collision_plan_dump(
    backend: wrela::query_plan::DispatchBackend,
) -> CollisionPlanCatalogDump {
    let plans = wrela::collision_plan::collision_plans_with_backend(backend)
        .iter()
        .map(collision_plan_dump_item)
        .collect();
    CollisionPlanCatalogDump {
        schema_version: wrela::collision_plan::COLLISION_PLAN_SCHEMA_VERSION,
        backend: dispatch_backend_name(backend).to_string(),
        plans,
    }
}

pub(crate) fn print_collision_plan_human(dump: &CollisionPlanCatalogDump) {
    println!("collision plan schema v{}", dump.schema_version);
    println!("backend: {}", dump.backend);
    if dump.plans.is_empty() {
        println!("plans: none");
        return;
    }
    for plan in &dump.plans {
        println!("plan {}", plan.name);
        println!(
            "  contract: {} v{} family={} question={} target={}",
            plan.contract_id, plan.contract_version, plan.family, plan.question, plan.target
        );
        println!("  authority_scope: {}", plan.authority_scope);
        println!(
            "  policy: backend_preference={} required_guarantee={} selected_method={}",
            plan.policy.backend_preference,
            plan.policy.required_guarantee,
            plan.policy.selected_method
        );
        println!(
            "  inputs: {}",
            if plan.inputs.is_empty() {
                "none".to_string()
            } else {
                plan.inputs
                    .iter()
                    .map(|input| format!("{}:{}({})", input.name, input.kind, input.record))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        );
        println!(
            "  artifacts: {}",
            if plan.artifacts.is_empty() {
                "none".to_string()
            } else {
                plan.artifacts
                    .iter()
                    .map(|artifact| format!(
                        "{} kind={} record={} contract={} contract_kind={} schema={} snapshot={} validity={}",
                        artifact.id,
                        artifact.kind,
                        artifact.record,
                        artifact.contract.id,
                        artifact.contract.kind,
                        artifact.contract.logical_schema,
                        artifact.contract.snapshot_relation,
                        artifact.contract.validity,
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        );
        println!(
            "  artifact uses: {}",
            if plan.artifact_uses.is_empty() {
                "none".to_string()
            } else {
                plan.artifact_uses
                    .iter()
                    .map(|use_record| {
                        let required_validity = use_record
                            .required_validity
                            .clone()
                            .unwrap_or_else(|| "none".to_string());
                        format!(
                            "{}:{} kind={} source={} required_validity={}",
                            use_record.actor,
                            use_record.artifact_id,
                            use_record.kind,
                            use_record.source,
                            required_validity
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        );
        println!(
            "  outputs: {}",
            if plan.outputs.is_empty() {
                "none".to_string()
            } else {
                plan.outputs
                    .iter()
                    .map(|output| {
                        let witness = output
                            .witness_schema
                            .as_ref()
                            .map(|schema| schema.name.as_str())
                            .unwrap_or("none");
                        format!(
                            "{}:{}({}) witness={}",
                            output.name, output.kind, output.record, witness
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        );
        println!("  passes:");
        for pass in &plan.passes {
            println!(
                "    {} kind={} consumes={} materializes={} queries={}",
                pass.id,
                pass.kind,
                if pass.consumes.is_empty() {
                    "none".to_string()
                } else {
                    pass.consumes.join(", ")
                },
                if pass.materializes.is_empty() {
                    "none".to_string()
                } else {
                    pass.materializes.join(", ")
                },
                if pass.query_dependencies.is_empty() {
                    "none".to_string()
                } else {
                    pass.query_dependencies.join(", ")
                }
            );
        }
        print_observer_projection_human(&plan.observer_projection);
        println!("  validation: {}", plan.validation.status);
        for err in &plan.validation.errors {
            println!("    - {}", err);
        }
    }
}

pub(crate) fn collision_contract_dump(
    descriptor: &wrela::collision_contract::CollisionContractDescriptor,
) -> CollisionContractCatalogItemDump {
    CollisionContractCatalogItemDump {
        contract_id: descriptor.id.as_str().to_string(),
        contract_version: descriptor.version,
        family: wrela::collision_contract::collision_family_name(descriptor.family).to_string(),
        question: wrela::collision_contract::collision_question_name(descriptor.question)
            .to_string(),
        target: wrela::collision_contract::collision_target_name(descriptor.target).to_string(),
        authority: collision_authority_requirement_dump(descriptor.authority),
        input_kind: wrela::collision_contract::collision_input_kind_name(descriptor.input_kind)
            .to_string(),
        input_record: descriptor.input_record.to_string(),
        output_kind: wrela::collision_contract::collision_output_kind_name(descriptor.output_kind)
            .to_string(),
        output_record: descriptor.output_record.to_string(),
        witness_schema: collision_witness_schema_dump(descriptor.witness_schema),
        policy: collision_execution_policy_dump(descriptor.policy),
        backends: wrela::collision_contract::collision_backend_support_names(
            descriptor.supported_backends,
        )
        .into_iter()
        .map(str::to_string)
        .collect(),
    }
}

pub(crate) fn collision_plan_dump_item(
    plan: &wrela::collision_plan::CollisionPlan,
) -> CollisionPlanDumpItem {
    let validation = observer_validation_summary(
        plan.validate()
            .into_iter()
            .map(|err| err.message.to_string()),
    );
    CollisionPlanDumpItem {
        name: plan.name.to_string(),
        contract_id: plan.contract_id.as_str().to_string(),
        contract_version: plan.contract_version,
        family: wrela::collision_contract::collision_family_name(plan.family).to_string(),
        question: wrela::collision_contract::collision_question_name(plan.question).to_string(),
        target: wrela::collision_contract::collision_target_name(plan.target).to_string(),
        authority_scope: wrela::collision_contract::collision_authority_scope_name(
            plan.authority_scope,
        )
        .to_string(),
        backend: dispatch_backend_name(plan.backend).to_string(),
        policy: collision_execution_policy_dump(plan.policy),
        inputs: plan
            .inputs
            .iter()
            .map(|input| CollisionPlanInputDump {
                name: input.name.to_string(),
                kind: wrela::collision_contract::collision_input_kind_name(input.kind).to_string(),
                record: input.record.to_string(),
            })
            .collect(),
        passes: plan
            .passes
            .iter()
            .map(|pass| CollisionPlanPassDump {
                id: pass.id.to_string(),
                kind: collision_pass_kind_name(&pass.kind),
                consumes: pass.consumes.iter().map(ToString::to_string).collect(),
                materializes: pass.materializes.iter().map(ToString::to_string).collect(),
                query_dependencies: pass
                    .kind
                    .query_dependencies()
                    .iter()
                    .map(|dependency| dependency.as_str().to_string())
                    .collect(),
            })
            .collect(),
        artifacts: plan
            .artifacts
            .iter()
            .cloned()
            .map(collision_artifact_binding_dump)
            .collect(),
        artifact_uses: plan
            .artifact_uses()
            .into_iter()
            .map(observer_artifact_use_dump)
            .collect(),
        outputs: plan
            .outputs
            .iter()
            .map(|output| CollisionPlanOutputDump {
                name: output.name.to_string(),
                kind: wrela::collision_contract::collision_output_kind_name(output.kind)
                    .to_string(),
                record: output.record.to_string(),
                witness_schema: output.witness_schema.map(collision_witness_schema_dump),
            })
            .collect(),
        observer_projection: query_program_debug::observer_projection_for_collision_plan(plan),
        validation,
    }
}

pub(crate) fn collision_artifact_binding_dump(
    binding: wrela::collision_plan::CollisionArtifactBinding,
) -> CollisionArtifactBindingDump {
    CollisionArtifactBindingDump {
        id: binding.id.to_string(),
        kind: wrela::collision_plan::collision_artifact_kind_name(binding.kind).to_string(),
        record: binding.record.to_string(),
        contract: observer_semantic_artifact_dump(binding.contract),
    }
}

pub(crate) fn collision_witness_schema_dump(
    schema: &wrela::collision_contract::CollisionWitnessSchema,
) -> CollisionWitnessSchemaDump {
    CollisionWitnessSchemaDump {
        name: schema.name.to_string(),
        kind: wrela::collision_contract::collision_witness_kind_name(schema.kind).to_string(),
        fields: schema
            .fields
            .iter()
            .map(|field| CollisionWitnessFieldDump {
                name: field.name.to_string(),
                ty: field.ty.to_string(),
            })
            .collect(),
    }
}

pub(crate) fn collision_execution_policy_dump(
    policy: wrela::collision_contract::CollisionExecutionPolicy,
) -> CollisionExecutionPolicyDump {
    CollisionExecutionPolicyDump {
        backend_preference: dispatch_backend_name(policy.backend_preference).to_string(),
        required_guarantee: policy.required_guarantee.name().to_string(),
        selected_method: policy.selected_method.name().to_string(),
    }
}

pub(crate) fn collision_authority_requirement_dump(
    authority: wrela::collision_contract::CollisionAuthorityRequirement,
) -> CollisionAuthorityRequirementDump {
    CollisionAuthorityRequirementDump {
        scope: wrela::collision_contract::collision_authority_scope_name(authority.scope)
            .to_string(),
        requires_previous_snapshot: authority.requires_previous_snapshot,
        required_evidence_scope: format!("{:?}", authority.required_evidence_scope),
        transition_compatibility: authority
            .transition_compatibility
            .map(|compatibility| format!("{compatibility:?}")),
    }
}

pub(crate) fn collision_run_report(
    backend: wrela::query_plan::DispatchBackend,
) -> Result<CollisionRunReport, String> {
    let query_ctx = collision_demo_context()?;
    let scene_id =
        wrela::query_exec::stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = collision_demo_domain(scene_id);
    let point = collision_demo_point([0.0, 0.0, 0.25]);
    let ray = collision_demo_ray([0.0, 0.0, 2.5], [0.0, 0.0, -1.0]);
    let overlap = collision_demo_probe([0.15, 0.0, 0.25], 0.35);
    let sweep = collision_demo_sweep([0.0, 0.0, 2.0], [0.0, 0.0, -2.0], 0.25);
    let toi = collision_demo_sweep([0.0, 0.0, 2.4], [0.0, 0.0, -1.6], 0.20);

    let point_plan = wrela::collision_plan::CollisionPlan::for_query_with_backend(
        wrela::collision_plan::CollisionQueryKind::PointOccupancyWorld,
        backend,
    );
    let ray_plan = wrela::collision_plan::CollisionPlan::for_query_with_backend(
        wrela::collision_plan::CollisionQueryKind::RayCastWorld,
        backend,
    );
    let overlap_plan = wrela::collision_plan::CollisionPlan::for_query_with_backend(
        wrela::collision_plan::CollisionQueryKind::SphereOverlapWorld,
        backend,
    );
    let sweep_plan = wrela::collision_plan::CollisionPlan::for_query_with_backend(
        wrela::collision_plan::CollisionQueryKind::SphereSweepTransition,
        backend,
    );
    let toi_plan = wrela::collision_plan::CollisionPlan::for_query_with_backend(
        wrela::collision_plan::CollisionQueryKind::SphereTimeOfImpactTransition,
        backend,
    );
    let mut store = wrela::collision_exec::cpu::CollisionArtifactStore::default();

    let point_started = Instant::now();
    let point_result = point_plan
        .execute(
            &query_ctx,
            &[collision_demo_capture(scene_id, 2), domain.clone(), point],
        )
        .map_err(|err| err.to_string())?;
    let point_runtime_ns = point_started.elapsed().as_nanos();

    let ray_started = Instant::now();
    let ray_result = ray_plan
        .execute(
            &query_ctx,
            &[collision_demo_capture(scene_id, 2), domain.clone(), ray],
        )
        .map_err(|err| err.to_string())?;
    let ray_runtime_ns = ray_started.elapsed().as_nanos();

    let overlap_started = Instant::now();
    let overlap_result = overlap_plan
        .execute(
            &query_ctx,
            &[collision_demo_capture(scene_id, 2), domain.clone(), overlap],
        )
        .map_err(|err| err.to_string())?;
    let overlap_runtime_ns = overlap_started.elapsed().as_nanos();

    let first_transition_started = Instant::now();
    let first_transition_result = wrela::collision_exec::cpu::execute_with_store(
        &sweep_plan,
        &query_ctx,
        &[
            collision_demo_capture(scene_id, 1),
            domain.clone(),
            collision_demo_transition(1, 0, wrela::state_advance::ChangeClass::Presentation),
            sweep.clone(),
        ],
        &mut store,
    )
    .map_err(|err| err.to_string())?;
    let first_transition_runtime_ns = first_transition_started.elapsed().as_nanos();

    let second_transition_started = Instant::now();
    let second_transition_result = wrela::collision_exec::cpu::execute_with_store(
        &sweep_plan,
        &query_ctx,
        &[
            collision_demo_capture(scene_id, 2),
            domain.clone(),
            collision_demo_transition(2, 1, wrela::state_advance::ChangeClass::Presentation),
            sweep.clone(),
        ],
        &mut store,
    )
    .map_err(|err| err.to_string())?;
    let second_transition_runtime_ns = second_transition_started.elapsed().as_nanos();

    let third_transition_started = Instant::now();
    let third_transition_result = wrela::collision_exec::cpu::execute_with_store(
        &sweep_plan,
        &query_ctx,
        &[
            collision_demo_capture(scene_id, 3),
            domain.clone(),
            collision_demo_transition(3, 1, wrela::state_advance::ChangeClass::Presentation),
            sweep,
        ],
        &mut store,
    )
    .map_err(|err| err.to_string())?;
    let third_transition_runtime_ns = third_transition_started.elapsed().as_nanos();

    let first_toi_started = Instant::now();
    let first_toi_result = wrela::collision_exec::cpu::execute_with_store(
        &toi_plan,
        &query_ctx,
        &[
            collision_demo_capture(scene_id, 1),
            domain.clone(),
            collision_demo_transition(1, 0, wrela::state_advance::ChangeClass::Presentation),
            toi.clone(),
        ],
        &mut store,
    )
    .map_err(|err| err.to_string())?;
    let first_toi_runtime_ns = first_toi_started.elapsed().as_nanos();

    let second_toi_started = Instant::now();
    let second_toi_result = wrela::collision_exec::cpu::execute_with_store(
        &toi_plan,
        &query_ctx,
        &[
            collision_demo_capture(scene_id, 2),
            domain.clone(),
            collision_demo_transition(2, 1, wrela::state_advance::ChangeClass::Presentation),
            toi.clone(),
        ],
        &mut store,
    )
    .map_err(|err| err.to_string())?;
    let second_toi_runtime_ns = second_toi_started.elapsed().as_nanos();

    let third_toi_started = Instant::now();
    let third_toi_result = wrela::collision_exec::cpu::execute_with_store(
        &toi_plan,
        &query_ctx,
        &[
            collision_demo_capture(scene_id, 3),
            domain.clone(),
            collision_demo_transition(3, 1, wrela::state_advance::ChangeClass::Presentation),
            toi,
        ],
        &mut store,
    )
    .map_err(|err| err.to_string())?;
    let third_toi_runtime_ns = third_toi_started.elapsed().as_nanos();

    Ok(CollisionRunReport {
        schema_version: 1,
        backend: dispatch_backend_name(backend).to_string(),
        executions: vec![
            collision_execution_dump(
                "point-occupancy",
                &point_plan,
                point_result.0,
                point_result.1,
                point_runtime_ns,
            ),
            collision_execution_dump(
                "ray-cast-first",
                &ray_plan,
                ray_result.0,
                ray_result.1,
                ray_runtime_ns,
            ),
            collision_execution_dump(
                "sphere-overlap-burst",
                &overlap_plan,
                overlap_result.0,
                overlap_result.1,
                overlap_runtime_ns,
            ),
            collision_execution_dump(
                "sphere-sweep-first",
                &sweep_plan,
                first_transition_result.0,
                first_transition_result.1,
                first_transition_runtime_ns,
            ),
            collision_execution_dump(
                "sphere-sweep-reused",
                &sweep_plan,
                second_transition_result.0,
                second_transition_result.1,
                second_transition_runtime_ns,
            ),
            collision_execution_dump(
                "sphere-sweep-rejected",
                &sweep_plan,
                third_transition_result.0,
                third_transition_result.1,
                third_transition_runtime_ns,
            ),
            collision_execution_dump(
                "time-of-impact-first",
                &toi_plan,
                first_toi_result.0,
                first_toi_result.1,
                first_toi_runtime_ns,
            ),
            collision_execution_dump(
                "time-of-impact-reused",
                &toi_plan,
                second_toi_result.0,
                second_toi_result.1,
                second_toi_runtime_ns,
            ),
            collision_execution_dump(
                "time-of-impact-rejected",
                &toi_plan,
                third_toi_result.0,
                third_toi_result.1,
                third_toi_runtime_ns,
            ),
        ],
    })
}

pub(crate) fn collision_execution_dump(
    name: &str,
    plan: &wrela::collision_plan::CollisionPlan,
    result: wrela::collision_contract::CollisionResult,
    trace: wrela::collision_plan::CollisionExecutionTrace,
    runtime_ns: u128,
) -> CollisionExecutionDump {
    CollisionExecutionDump {
        name: name.to_string(),
        plan_name: plan.name.to_string(),
        contract_id: plan.contract_id.as_str().to_string(),
        target: wrela::collision_contract::collision_target_name(plan.target).to_string(),
        authority_scope: wrela::collision_contract::collision_authority_scope_name(
            plan.authority_scope,
        )
        .to_string(),
        runtime_ns,
        result: collision_result_dump(result),
        trace: collision_execution_trace_dump(trace),
    }
}

pub(crate) fn collision_result_dump(
    result: wrela::collision_contract::CollisionResult,
) -> CollisionResultDump {
    match result {
        wrela::collision_contract::CollisionResult::Occupancy(value) => {
            CollisionResultDump::Occupancy {
                occupied: value.occupied,
                classification: format!("{:?}", value.classification),
                signed_distance: value.signed_distance,
                witness: collision_point_witness_dump(value.witness),
            }
        }
        wrela::collision_contract::CollisionResult::RayCast(value) => {
            CollisionResultDump::RayCast {
                hit: value.hit,
                miss_reason: format!("{:?}", value.miss_reason),
                witness: value.witness.map(collision_ray_witness_dump),
            }
        }
        wrela::collision_contract::CollisionResult::SphereOverlap(value) => {
            CollisionResultDump::SphereOverlap {
                overlaps: value.overlaps,
                signed_separation: value.signed_separation,
                witness: collision_sphere_witness_dump(value.witness),
            }
        }
        wrela::collision_contract::CollisionResult::Sweep(value) => CollisionResultDump::Sweep {
            hit: value.hit,
            witness: value.witness.map(collision_sweep_witness_dump),
            no_hit_certificate: value
                .no_hit_certificate
                .map(collision_no_hit_certificate_dump),
        },
        wrela::collision_contract::CollisionResult::TimeOfImpact(value) => {
            CollisionResultDump::TimeOfImpact {
                hit: value.hit,
                time_fraction_upper_bound: value.time_fraction_upper_bound,
                witness: value.witness.map(collision_toi_witness_dump),
                no_hit_certificate: value
                    .no_hit_certificate
                    .map(collision_no_hit_certificate_dump),
            }
        }
    }
}

pub(crate) fn collision_point_witness_dump(
    witness: wrela::collision_contract::CollisionPointWitness,
) -> CollisionPointWitnessDump {
    CollisionPointWitnessDump {
        sample_point: witness.sample_point,
        nearest_point_on_world: witness.nearest_point_on_world,
        world_normal: witness.world_normal,
        signed_distance: witness.signed_distance,
        normal_provenance: wrela::collision_contract::collision_contact_normal_provenance_name(
            witness.normal_provenance,
        )
        .to_string(),
    }
}

pub(crate) fn collision_ray_witness_dump(
    witness: wrela::collision_contract::CollisionRayWitness,
) -> CollisionRayWitnessDump {
    CollisionRayWitnessDump {
        travel_distance: witness.travel_distance,
        position: witness.position,
        normal: witness.normal,
        root_shape_id: witness.root_shape_id,
        feature_id: witness.feature_id,
        normal_provenance: wrela::collision_contract::collision_contact_normal_provenance_name(
            witness.normal_provenance,
        )
        .to_string(),
    }
}

pub(crate) fn collision_sphere_witness_dump(
    witness: wrela::collision_contract::CollisionSphereWitness,
) -> CollisionSphereWitnessDump {
    CollisionSphereWitnessDump {
        point_on_probe: witness.point_on_probe,
        point_on_world: witness.point_on_world,
        world_normal: witness.world_normal,
        signed_separation: witness.signed_separation,
        normal_provenance: wrela::collision_contract::collision_contact_normal_provenance_name(
            witness.normal_provenance,
        )
        .to_string(),
    }
}

pub(crate) fn collision_sweep_witness_dump(
    witness: wrela::collision_contract::CollisionSweepWitness,
) -> CollisionSweepWitnessDump {
    CollisionSweepWitnessDump {
        contact_fraction_upper_bound: witness.contact_fraction_upper_bound,
        point_on_probe: witness.point_on_probe,
        point_on_world: witness.point_on_world,
        contact_normal: witness.contact_normal,
        normal_flavor: wrela::collision_contract::collision_contact_normal_flavor_name(
            witness.normal_flavor,
        )
        .to_string(),
        normal_provenance: wrela::collision_contract::collision_contact_normal_provenance_name(
            witness.normal_provenance,
        )
        .to_string(),
    }
}

pub(crate) fn collision_toi_witness_dump(
    witness: wrela::collision_contract::CollisionTimeOfImpactWitness,
) -> CollisionTimeOfImpactWitnessDump {
    CollisionTimeOfImpactWitnessDump {
        time_fraction_upper_bound: witness.time_fraction_upper_bound,
        point_on_probe: witness.point_on_probe,
        point_on_world: witness.point_on_world,
        contact_normal: witness.contact_normal,
        normal_flavor: wrela::collision_contract::collision_contact_normal_flavor_name(
            witness.normal_flavor,
        )
        .to_string(),
        normal_provenance: wrela::collision_contract::collision_contact_normal_provenance_name(
            witness.normal_provenance,
        )
        .to_string(),
    }
}

pub(crate) fn collision_no_hit_certificate_dump(
    certificate: wrela::collision_contract::CollisionNoHitCertificate,
) -> CollisionNoHitCertificateDump {
    CollisionNoHitCertificateDump {
        valid_through_fraction: certificate.valid_through_fraction,
        guarantee: certificate.guarantee.name().to_string(),
    }
}

pub(crate) fn collision_execution_trace_dump(
    trace: wrela::collision_plan::CollisionExecutionTrace,
) -> CollisionExecutionTraceDump {
    CollisionExecutionTraceDump {
        contract_id: trace.contract_id.as_str().to_string(),
        family: wrela::collision_contract::collision_family_name(trace.family).to_string(),
        question: wrela::collision_contract::collision_question_name(trace.question).to_string(),
        backend: dispatch_backend_name(trace.backend).to_string(),
        snapshot: trace.snapshot,
        transition: trace.transition.map(|transition| CollisionTransitionDump {
            current_snapshot_epoch: transition.current_snapshot_epoch,
            previous_snapshot_epoch: transition.previous_snapshot_epoch,
            change_class: format!("{:?}", transition.change_class),
        }),
        required_guarantee: trace.required_guarantee.name().to_string(),
        selected_method: trace.selected_method.name().to_string(),
        executed_query_contracts: trace
            .executed_query_contracts
            .iter()
            .map(|contract| contract.as_str().to_string())
            .collect(),
        broadphase_candidate_count: trace.broadphase_candidate_count,
        broadphase_rejected_candidate_count: trace.broadphase_rejected_candidate_count,
        broadphase_pruned_node_count: trace.broadphase_pruned_node_count,
        interval_bracket: trace.interval_bracket,
        interval_subdivisions: trace.interval_subdivisions,
        interval_refinements: trace.interval_refinements,
        certificate_successes: trace.certificate_successes,
        fallback_count: trace.fallback_count,
        contact_normal_provenance: trace.contact_normal_provenance.map(|provenance| {
            wrela::collision_contract::collision_contact_normal_provenance_name(provenance)
                .to_string()
        }),
        reuse_metrics: CollisionReuseMetricsDump {
            available_count: trace.reuse_metrics.available_count,
            consumed_count: trace.reuse_metrics.consumed_count,
            rejected_count: trace.reuse_metrics.rejected_count,
            unavailable_count: trace.reuse_metrics.unavailable_count,
            diagnostics: trace.reuse_metrics.diagnostics,
        },
        reuse_decisions: trace
            .reuse_decisions
            .into_iter()
            .map(|decision| CollisionReuseDecisionDump {
                artifact_id: decision.artifact_id.to_string(),
                kind: wrela::collision_plan::collision_artifact_kind_name(decision.artifact_kind)
                    .to_string(),
                verdict: wrela::collision_plan::collision_reuse_verdict_name(decision.verdict)
                    .to_string(),
                reason: wrela::collision_plan::collision_reuse_reason_name(decision.reason)
                    .to_string(),
                detail: decision.detail.to_string(),
            })
            .collect(),
        wgsl_metrics: trace.wgsl_metrics.map(|metrics| CollisionWgslMetricsDump {
            dispatch_count: metrics.dispatch_count,
            dispatch_items: metrics.dispatch_items,
            candidate_reduction_effectiveness: metrics.candidate_reduction_effectiveness,
            selected_workgroup_size: metrics.selected_workgroup_size,
            resident_shared_snapshot_artifacts: metrics.resident_shared_snapshot_artifacts,
            cpu_certification_query_count: metrics.cpu_certification_query_count,
        }),
    }
}

pub(crate) fn print_collision_run_human(report: &CollisionRunReport) {
    println!("collision run schema v{}", report.schema_version);
    println!("backend: {}", report.backend);
    for execution in &report.executions {
        println!("execution {}", execution.name);
        println!(
            "  plan: {} target={} authority_scope={} contract={}",
            execution.plan_name, execution.target, execution.authority_scope, execution.contract_id
        );
        println!("  result: {}", collision_result_human(&execution.result));
        println!(
            "  trace: contract={} family={} question={} backend={} required_guarantee={} selected_method={}",
            execution.trace.contract_id,
            execution.trace.family,
            execution.trace.question,
            execution.trace.backend,
            execution.trace.required_guarantee,
            execution.trace.selected_method
        );
        if let Some(snapshot) = &execution.trace.snapshot {
            println!(
                "    snapshot: {} snapshot_id={} epoch={} portable_scene_id={}",
                snapshot.capture_name,
                snapshot.snapshot_id.0,
                snapshot.epoch.0,
                snapshot.portable_scene_id
            );
        }
        if let Some(transition) = &execution.trace.transition {
            println!(
                "    transition: current_epoch={} previous_epoch={} change_class={}",
                transition.current_snapshot_epoch,
                transition.previous_snapshot_epoch,
                transition.change_class
            );
        }
        if !execution.trace.executed_query_contracts.is_empty() {
            println!(
                "    query contracts: {}",
                execution.trace.executed_query_contracts.join(", ")
            );
        }
        println!(
            "    broadphase: candidate_count={} rejected_candidate_count={} pruned_node_count={} interval_subdivisions={} interval_refinements={} certificate_successes={} fallback_count={}",
            execution.trace.broadphase_candidate_count,
            execution.trace.broadphase_rejected_candidate_count,
            execution.trace.broadphase_pruned_node_count,
            execution.trace.interval_subdivisions,
            execution.trace.interval_refinements,
            execution.trace.certificate_successes,
            execution.trace.fallback_count
        );
        if let Some(bracket) = execution.trace.interval_bracket {
            println!(
                "    interval bracket: [{:.6}, {:.6}]",
                bracket[0], bracket[1]
            );
        }
        if let Some(provenance) = &execution.trace.contact_normal_provenance {
            println!("    contact normal provenance: {}", provenance);
        }
        if let Some(metrics) = &execution.trace.wgsl_metrics {
            println!(
                "    wgsl: dispatch_count={} dispatch_items={} candidate_reduction_effectiveness={:.3} selected_workgroup_size={} resident_shared_snapshot_artifacts={} cpu_certification_query_count={}",
                metrics.dispatch_count,
                metrics.dispatch_items,
                metrics.candidate_reduction_effectiveness,
                metrics.selected_workgroup_size,
                metrics.resident_shared_snapshot_artifacts,
                metrics.cpu_certification_query_count
            );
        }
        println!(
            "    reuse metrics: available={} consumed={} rejected={} unavailable={}",
            execution.trace.reuse_metrics.available_count,
            execution.trace.reuse_metrics.consumed_count,
            execution.trace.reuse_metrics.rejected_count,
            execution.trace.reuse_metrics.unavailable_count
        );
        if !execution.trace.reuse_metrics.diagnostics.is_empty() {
            println!("    reuse diagnostics:");
            for diagnostic in &execution.trace.reuse_metrics.diagnostics {
                println!("      - {}", diagnostic);
            }
        }
        if !execution.trace.reuse_decisions.is_empty() {
            println!("    reuse decisions:");
            for decision in &execution.trace.reuse_decisions {
                println!(
                    "      - artifact={} kind={} verdict={} reason={} detail={}",
                    decision.artifact_id,
                    decision.kind,
                    decision.verdict,
                    decision.reason,
                    decision.detail
                );
            }
        }
        println!("    runtime_ns={}", execution.runtime_ns);
    }
}

pub(crate) fn collision_result_human(result: &CollisionResultDump) -> String {
    match result {
        CollisionResultDump::Occupancy {
            occupied,
            classification,
            signed_distance,
            witness,
        } => format!(
            "occupancy occupied={} classification={} signed_distance={} witness=sample_point={:?} world_normal={:?} normal_provenance={}",
            occupied,
            classification,
            signed_distance,
            witness.sample_point,
            witness.world_normal,
            witness.normal_provenance
        ),
        CollisionResultDump::RayCast {
            hit,
            miss_reason,
            witness,
        } => format!(
            "ray_cast hit={} miss_reason={} witness={}",
            hit,
            miss_reason,
            witness
                .as_ref()
                .map(|w| format!(
                    "travel_distance={} position={:?} normal_provenance={}",
                    w.travel_distance, w.position, w.normal_provenance
                ))
                .unwrap_or_else(|| "none".to_string())
        ),
        CollisionResultDump::SphereOverlap {
            overlaps,
            signed_separation,
            witness,
        } => format!(
            "sphere_overlap overlaps={} signed_separation={} witness=point_on_probe={:?} world_normal={:?} normal_provenance={}",
            overlaps,
            signed_separation,
            witness.point_on_probe,
            witness.world_normal,
            witness.normal_provenance
        ),
        CollisionResultDump::Sweep {
            hit,
            witness,
            no_hit_certificate,
        } => format!(
            "sweep hit={} witness={} no_hit_certificate={}",
            hit,
            witness
                .as_ref()
                .map(|w| format!(
                    "fraction={} normal_flavor={} normal_provenance={} point_on_probe={:?}",
                    w.contact_fraction_upper_bound,
                    w.normal_flavor,
                    w.normal_provenance,
                    w.point_on_probe
                ))
                .unwrap_or_else(|| "none".to_string()),
            no_hit_certificate
                .as_ref()
                .map(|certificate| format!(
                    "valid_through_fraction={} guarantee={}",
                    certificate.valid_through_fraction, certificate.guarantee
                ))
                .unwrap_or_else(|| "none".to_string())
        ),
        CollisionResultDump::TimeOfImpact {
            hit,
            time_fraction_upper_bound,
            witness,
            no_hit_certificate,
        } => format!(
            "time_of_impact hit={} upper_bound={:?} witness={} no_hit_certificate={}",
            hit,
            time_fraction_upper_bound,
            witness
                .as_ref()
                .map(|w| format!(
                    "fraction={} normal_flavor={} normal_provenance={} point_on_probe={:?}",
                    w.time_fraction_upper_bound,
                    w.normal_flavor,
                    w.normal_provenance,
                    w.point_on_probe
                ))
                .unwrap_or_else(|| "none".to_string()),
            no_hit_certificate
                .as_ref()
                .map(|certificate| format!(
                    "valid_through_fraction={} guarantee={}",
                    certificate.valid_through_fraction, certificate.guarantee
                ))
                .unwrap_or_else(|| "none".to_string())
        ),
    }
}

pub(crate) fn collision_demo_context() -> Result<wrela::query_exec::QueryExecContext, String> {
    let node = parser::parse(collision_demo_source());
    let root =
        ast::Root::cast(node).ok_or_else(|| "collision demo source did not parse".to_string())?;
    let module = hir_lower::lower(root);
    let semantic = hir::semantic::check_module(&module);
    if !semantic.errors.is_empty() {
        return Err(format!(
            "collision demo semantic errors: {:?}",
            semantic.errors
        ));
    }
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    if !type_errors.is_empty() {
        return Err(format!("collision demo type errors: {type_errors:?}"));
    }
    Ok(wrela::query_exec::QueryExecContext::compile(
        &module, &type_info,
    ))
}

pub(crate) fn collision_demo_source() -> &'static str {
    r#"
field exact distance collision_field(p: Vec3) -> F32 {
    sphere(radius = 0.5)
}

field exact distance collision_left_field(p: Vec3) -> F32 {
    translate = vec3(-2.5, 0.15, 0.0) {
        use collision_field
    }
}

field exact distance collision_right_field(p: Vec3) -> F32 {
    translate = vec3(2.3, -0.2, 0.0) {
        use collision_field
    }
}

material collision_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.8, 0.3, 0.2),
        roughness=0.2,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape collision_shape {
    field = collision_field
    material = collision_surface
}

shape collision_left_shape {
    field = collision_left_field
    material = collision_surface
}

shape collision_right_shape {
    field = collision_right_field
    material = collision_surface
}

region collision_region() {
    place sample = collision_shape
    place left = collision_left_shape
    place right = collision_right_shape
}

domain collision_domain(world: RegionCapture) {
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

pub(crate) fn collision_demo_domain(scene_id: u32) -> wrela::kernel::KernelValue {
    wrela::kernel::KernelValue::Struct(wrela::kernel::KernelStructValue {
        name: SmolStr::new("SceneDomain"),
        fields: vec![
            (
                SmolStr::new("scene_id"),
                wrela::kernel::KernelValue::U32(scene_id),
            ),
            (
                SmolStr::new("spatial"),
                wrela::kernel::KernelValue::Struct(wrela::kernel::KernelStructValue {
                    name: SmolStr::new("SpatialDomainContract"),
                    fields: vec![(
                        SmolStr::new("geometry_detail"),
                        wrela::kernel::KernelValue::I32(1),
                    )],
                }),
            ),
            (
                SmolStr::new("surface"),
                wrela::kernel::KernelValue::Struct(wrela::kernel::KernelStructValue {
                    name: SmolStr::new("SurfaceDomainContract"),
                    fields: vec![(
                        SmolStr::new("material"),
                        wrela::kernel::KernelValue::Bool(true),
                    )],
                }),
            ),
            (
                SmolStr::new("participants"),
                wrela::kernel::KernelValue::Struct(wrela::kernel::KernelStructValue {
                    name: SmolStr::new("ParticipantDomainContract"),
                    fields: vec![
                        (
                            SmolStr::new("radiance"),
                            wrela::kernel::KernelValue::Bool(false),
                        ),
                        (
                            SmolStr::new("media"),
                            wrela::kernel::KernelValue::Bool(false),
                        ),
                    ],
                }),
            ),
        ],
    })
}

pub(crate) fn collision_demo_transition(
    current_epoch: u32,
    previous_epoch: u32,
    change_class: wrela::state_advance::ChangeClass,
) -> wrela::kernel::KernelValue {
    let change_class_id = match change_class {
        wrela::state_advance::ChangeClass::None => 0,
        wrela::state_advance::ChangeClass::Presentation => 1,
        wrela::state_advance::ChangeClass::Structural => 2,
        wrela::state_advance::ChangeClass::Topology => 3,
        wrela::state_advance::ChangeClass::Identity => 4,
        wrela::state_advance::ChangeClass::Behavior => 5,
        wrela::state_advance::ChangeClass::Incompatible => 6,
    };
    wrela::kernel::KernelValue::Struct(wrela::kernel::KernelStructValue {
        name: SmolStr::new("CollisionSnapshotTransitionInput"),
        fields: vec![
            (
                SmolStr::new("current_snapshot_epoch"),
                wrela::kernel::KernelValue::U32(current_epoch),
            ),
            (
                SmolStr::new("previous_snapshot_epoch"),
                wrela::kernel::KernelValue::U32(previous_epoch),
            ),
            (
                SmolStr::new("change_class"),
                wrela::kernel::KernelValue::U32(change_class_id),
            ),
        ],
    })
}

pub(crate) fn collision_demo_capture(scene_id: u32, epoch: u32) -> wrela::kernel::KernelValue {
    wrela::kernel::KernelValue::Struct(wrela::kernel::KernelStructValue {
        name: SmolStr::new("RegionCapture"),
        fields: vec![
            (
                SmolStr::new("scene_id"),
                wrela::kernel::KernelValue::U32(scene_id),
            ),
            (
                SmolStr::new("epoch"),
                wrela::kernel::KernelValue::U32(epoch),
            ),
        ],
    })
}

pub(crate) fn collision_demo_point(point: [f32; 3]) -> wrela::kernel::KernelValue {
    wrela::kernel::KernelValue::Struct(wrela::kernel::KernelStructValue {
        name: SmolStr::new("CollisionPointInput"),
        fields: vec![(
            SmolStr::new("point"),
            wrela::kernel::KernelValue::Vec3(point),
        )],
    })
}

pub(crate) fn collision_demo_ray(
    origin: [f32; 3],
    direction: [f32; 3],
) -> wrela::kernel::KernelValue {
    wrela::kernel::KernelValue::Struct(wrela::kernel::KernelStructValue {
        name: SmolStr::new("CollisionRayInput"),
        fields: vec![
            (
                SmolStr::new("origin"),
                wrela::kernel::KernelValue::Vec3(origin),
            ),
            (
                SmolStr::new("direction"),
                wrela::kernel::KernelValue::Vec3(direction),
            ),
            (
                SmolStr::new("max_distance"),
                wrela::kernel::KernelValue::F32(8.0),
            ),
            (
                SmolStr::new("min_step"),
                wrela::kernel::KernelValue::F32(0.05),
            ),
            (
                SmolStr::new("hit_epsilon"),
                wrela::kernel::KernelValue::F32(0.001),
            ),
            (
                SmolStr::new("max_steps"),
                wrela::kernel::KernelValue::I32(96),
            ),
        ],
    })
}

pub(crate) fn collision_demo_probe(center: [f32; 3], radius: f32) -> wrela::kernel::KernelValue {
    wrela::kernel::KernelValue::Struct(wrela::kernel::KernelStructValue {
        name: SmolStr::new("CollisionSphereProbe"),
        fields: vec![
            (
                SmolStr::new("center"),
                wrela::kernel::KernelValue::Vec3(center),
            ),
            (
                SmolStr::new("radius"),
                wrela::kernel::KernelValue::F32(radius),
            ),
        ],
    })
}

pub(crate) fn collision_demo_sweep(
    start_center: [f32; 3],
    end_center: [f32; 3],
    radius: f32,
) -> wrela::kernel::KernelValue {
    wrela::kernel::KernelValue::Struct(wrela::kernel::KernelStructValue {
        name: SmolStr::new("CollisionSphereSweepInput"),
        fields: vec![
            (
                SmolStr::new("start_center"),
                wrela::kernel::KernelValue::Vec3(start_center),
            ),
            (
                SmolStr::new("end_center"),
                wrela::kernel::KernelValue::Vec3(end_center),
            ),
            (
                SmolStr::new("radius"),
                wrela::kernel::KernelValue::F32(radius),
            ),
            (
                SmolStr::new("contact_tolerance"),
                wrela::kernel::KernelValue::F32(0.001),
            ),
            (
                SmolStr::new("max_iterations"),
                wrela::kernel::KernelValue::I32(64),
            ),
        ],
    })
}
