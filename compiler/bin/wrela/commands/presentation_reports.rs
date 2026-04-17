//! Owns the typed report/dump projections for presentation plans, frame
//! contracts, preview output, and debug execution.
//! Does not own presentation execution or CLI argument parsing.
//!
//! Key invariants:
//! - report structs must reflect the executed plan/backend state that command
//!   handlers hand them.
//! - human-readable rendering and JSON dumps are alternate projections of the
//!   same typed report data.
//! - catalog names for attachments, reuse policy, and observability stay stable
//!   because downstream docs/tests treat them as report surface.
//!
//! Primary entrypoints:
//! - `presentation_plan_dump`
//! - `presentation_plan_dump_item`
//! - `print_presentation_plan_human`
//!
//! Failure modes / common pitfalls:
//! - re-deriving names from ad hoc strings here instead of shared enums makes
//!   report output drift from runtime truth.
//! - mixing execution-side lookups into this file obscures whether a bug lives in
//!   the runtime or the projection layer.

use super::observer_projection::*;
use super::*;

#[derive(Serialize)]
pub(crate) struct PresentationPlanDump {
    pub(crate) schema_version: u32,
    pub(crate) entry_path: String,
    pub(crate) plans: Vec<PresentationPlanDumpItem>,
}

#[derive(Serialize)]
pub(crate) struct PresentationDebugDump {
    pub(crate) schema_version: u32,
    pub(crate) view: String,
    pub(crate) region: String,
    pub(crate) domain: String,
    pub(crate) query_trace_solver_mode: String,
    pub(crate) backend: String,
    pub(crate) semantic_domain: String,
    pub(crate) execution_policy: String,
    pub(crate) snapshot: wrela::world_identity::SnapshotIdentityReport,
    pub(crate) frames_executed: u32,
    pub(crate) color_ppm: Option<String>,
    pub(crate) depth_ppm: Option<String>,
    pub(crate) world_normal_ppm: Option<String>,
    pub(crate) stats_path: String,
    pub(crate) stats: String,
    pub(crate) frame_cost: wrela::presentation_exec::PresentationFrameCostReport,
    pub(crate) frame_cost_history: Vec<wrela::presentation_exec::PresentationFrameCostReport>,
}

#[derive(Serialize)]
pub(crate) struct FrameContractsDump {
    pub(crate) schema_version: u32,
    pub(crate) entry_path: String,
    pub(crate) views: Vec<FrameContractsDumpItem>,
}

#[derive(Serialize)]
pub(crate) struct FrameContractsDumpItem {
    pub(crate) name: String,
    pub(crate) frame: PresentationFrameDump,
    pub(crate) frame_artifacts: Vec<PresentationFrameArtifactDump>,
    pub(crate) bindings: Vec<PresentationBindingDump>,
}

#[derive(Serialize)]
pub(crate) struct PreviewReportDump {
    pub(crate) schema_version: u32,
    pub(crate) view: String,
    pub(crate) region: String,
    pub(crate) domain: String,
    pub(crate) attachment: String,
    pub(crate) backend: String,
    pub(crate) semantic_domain: String,
    pub(crate) execution_policy: String,
    pub(crate) snapshot: wrela::world_identity::SnapshotIdentityReport,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) stats: String,
    pub(crate) frame_cost: wrela::presentation_exec::PresentationFrameCostReport,
}

#[derive(Serialize)]
pub(crate) struct FrameBundleDump {
    pub(crate) schema_version: u32,
    pub(crate) view: String,
    pub(crate) region: String,
    pub(crate) domain: String,
    pub(crate) backend: String,
    pub(crate) semantic_domain: String,
    pub(crate) execution_policy: String,
    pub(crate) snapshot: wrela::world_identity::SnapshotIdentityReport,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) frame_index: u32,
    pub(crate) attachments: Vec<serde_json::Value>,
    pub(crate) frame_cost: wrela::presentation_exec::PresentationFrameCostReport,
}

#[derive(Serialize)]
pub(crate) struct PresentationPlanDumpItem {
    pub(crate) name: String,
    pub(crate) view: PresentationViewDump,
    pub(crate) frame: PresentationFrameDump,
    pub(crate) passes: Vec<PresentationPassDump>,
    pub(crate) frame_artifacts: Vec<PresentationFrameArtifactDump>,
    pub(crate) semantic_artifacts: Vec<ObserverSemanticArtifactDump>,
    pub(crate) artifact_uses: Vec<ObserverArtifactUseDump>,
    pub(crate) bindings: Vec<PresentationBindingDump>,
    pub(crate) observer_projection: query_program_debug::ObserverProjectionDump,
    pub(crate) normalized_projection: query_program_debug::NormalizedCurrentPlanProjection,
    pub(crate) validation: ObserverValidationSummaryDump,
}

#[derive(Serialize)]
pub(crate) struct PresentationViewDump {
    pub(crate) canonical_projection: bool,
    pub(crate) canonical_projection_input: String,
    pub(crate) screen_lattice: PresentationScreenLatticeDump,
    pub(crate) canonical_view_ray: PresentationViewRayDump,
    pub(crate) allows_legacy_projection_override: bool,
    pub(crate) compatibility_projection: PresentationCompatibilityProjectionDump,
}

#[derive(Serialize)]
pub(crate) struct PresentationScreenLatticeDump {
    pub(crate) sample_position: String,
    pub(crate) origin: String,
    pub(crate) width_source: String,
    pub(crate) height_source: String,
}

#[derive(Serialize)]
pub(crate) struct PresentationViewRayDump {
    pub(crate) space: String,
    pub(crate) normalized_direction: bool,
    pub(crate) projection_input: String,
}

#[derive(Serialize)]
pub(crate) struct PresentationCompatibilityProjectionDump {
    pub(crate) legacy_path_active: bool,
    pub(crate) authored_world_up_override: bool,
    pub(crate) authored_view_scale_override: bool,
}

#[derive(Serialize)]
pub(crate) struct PresentationFrameDump {
    pub(crate) outputs: Vec<PresentationAttachmentDump>,
    pub(crate) primary_hit: Option<PresentationPrimaryHitDump>,
    pub(crate) temporal_reuse: Option<String>,
    pub(crate) temporal_change_class: Option<String>,
    pub(crate) quality: PresentationQualityDump,
    pub(crate) lighting: PresentationLightingDump,
    pub(crate) observability: Vec<String>,
}

#[derive(Serialize)]
pub(crate) struct PresentationQualityDump {
    pub(crate) tier: String,
    pub(crate) target_fps: u32,
    pub(crate) internal_resolution_scale: f32,
    pub(crate) allow_dynamic_resolution: bool,
    pub(crate) primary_max_steps: i32,
    pub(crate) allow_radiance: bool,
    pub(crate) allow_media: bool,
    pub(crate) temporal_mode: String,
    pub(crate) allow_half_res_participants: bool,
    pub(crate) allow_hit_compaction: bool,
    pub(crate) degradation_order: Vec<String>,
}

#[derive(Serialize)]
pub(crate) struct PresentationPrimaryHitDump {
    pub(crate) attachment: String,
    pub(crate) record: String,
    pub(crate) fields: Vec<String>,
    pub(crate) depth_semantics: String,
    pub(crate) sample_identity: String,
}

#[derive(Serialize)]
pub(crate) struct PresentationAttachmentDump {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) element_schema: String,
    pub(crate) lifetime: String,
    pub(crate) resolution: String,
    pub(crate) scale: String,
    pub(crate) clear_policy: String,
}

#[derive(Serialize)]
pub(crate) struct PresentationLightingDump {
    pub(crate) key_light: PresentationLightingInputDump,
    pub(crate) fill_direction: PresentationLightingInputDump,
    pub(crate) fill_strength: PresentationLightingInputDump,
    pub(crate) ambient_color: PresentationLightingInputDump,
    pub(crate) allows_legacy_plural_lights_metadata: bool,
}

#[derive(Serialize)]
pub(crate) struct PresentationLightingInputDump {
    pub(crate) binding: String,
    pub(crate) element_schema: String,
    pub(crate) source: String,
    pub(crate) temporary_compatibility_alias: bool,
}

#[derive(Serialize)]
pub(crate) struct PresentationPassDump {
    pub(crate) id: String,
    pub(crate) kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) screen_samples: Option<PresentationScreenSamplePassDump>,
    pub(crate) consumes: Vec<String>,
    pub(crate) materializes: Vec<String>,
    pub(crate) binding: Option<String>,
    pub(crate) query_dependencies: Vec<PresentationQueryDependencyDump>,
    pub(crate) future_acceleration_hooks: Vec<String>,
    pub(crate) observability: Vec<String>,
}

#[derive(Serialize)]
pub(crate) struct PresentationScreenSamplePassDump {
    pub(crate) viewport_width_source: String,
    pub(crate) viewport_height_source: String,
    pub(crate) samples_per_pixel: u32,
    pub(crate) jitter_source: String,
    pub(crate) item_count_expression: String,
    pub(crate) output_item_record: String,
}

#[derive(Serialize)]
pub(crate) struct PresentationQueryDependencyDump {
    pub(crate) contract_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) question: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) surface: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) cardinality: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) call: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) evidence: Option<PresentationEvidenceDump>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) solver_diagnostics: Option<PresentationRaySolverDump>,
}

#[derive(Serialize)]
pub(crate) struct PresentationEvidenceDump {
    pub(crate) subject: String,
    pub(crate) origin: String,
    pub(crate) scope: String,
    pub(crate) refinement_path: Vec<String>,
    pub(crate) distance_refinement_path: Vec<String>,
    pub(crate) support_refinement_path: Vec<String>,
    pub(crate) differential_refinement_path: Vec<String>,
    pub(crate) identity_refinement_path: Vec<String>,
    pub(crate) temporal_refinement_path: Vec<String>,
    pub(crate) distance_semantics: String,
    pub(crate) support_class: String,
    pub(crate) support_lower_bound_pruning: String,
    pub(crate) support_conservative_bounds: String,
    pub(crate) lipschitz: String,
    pub(crate) analytic_intersection: String,
    pub(crate) derivative: String,
    pub(crate) stable_feature_id: bool,
    pub(crate) stable_instance_id: bool,
    pub(crate) stable_repeat_id: bool,
    pub(crate) temporal_stability: String,
    pub(crate) temporal_change_class: String,
    pub(crate) temporal_stationary: String,
    pub(crate) temporal_rigid_over_interval: String,
    pub(crate) temporal_topology_stable: String,
    pub(crate) temporal_bounded_velocity: String,
}

#[derive(Serialize)]
pub(crate) struct PresentationRaySolverDump {
    pub(crate) plan_id: String,
    pub(crate) subject: String,
    pub(crate) methods: Vec<String>,
    pub(crate) mixed_selections: Vec<PresentationRaySolverSelectionDump>,
    pub(crate) artifact_reuse_intents: Vec<PresentationRaySolverIntentDump>,
    pub(crate) continuation_intents: Vec<PresentationRaySolverIntentDump>,
    pub(crate) fallback: String,
    pub(crate) unavailable_facts: Vec<String>,
}

#[derive(Serialize)]
pub(crate) struct PresentationRaySolverSelectionDump {
    pub(crate) subject: String,
    pub(crate) candidate_class: String,
    pub(crate) method: String,
    pub(crate) required_guarantee: String,
    pub(crate) selected_method_class: String,
    pub(crate) evidence_policy_summary: String,
}

#[derive(Serialize)]
pub(crate) struct PresentationRaySolverIntentDump {
    pub(crate) selection: PresentationRaySolverSelectionDump,
    pub(crate) disposition: String,
    pub(crate) reasons: Vec<String>,
}

#[derive(Serialize)]
pub(crate) struct PresentationFrameArtifactDump {
    pub(crate) id: String,
    pub(crate) attachment: String,
    pub(crate) producer_pass: String,
    pub(crate) materialized: bool,
}

pub(crate) fn presentation_plan_dump(
    entry_path: &Path,
    plans: &[wrela::presentation_plan::PresentationPlan],
) -> PresentationPlanDump {
    PresentationPlanDump {
        schema_version: 1,
        entry_path: entry_path.display().to_string(),
        plans: plans.iter().map(presentation_plan_dump_item).collect(),
    }
}

pub(crate) fn presentation_plan_dump_item(
    plan: &wrela::presentation_plan::PresentationPlan,
) -> PresentationPlanDumpItem {
    let validation = observer_validation_summary(
        plan.validate()
            .into_iter()
            .map(|err| err.message.to_string()),
    );
    PresentationPlanDumpItem {
        name: plan.name.to_string(),
        view: PresentationViewDump {
            canonical_projection: plan.view.canonical_projection,
            canonical_projection_input: canonical_projection_input_name(
                plan.view.canonical_projection_input,
            ),
            screen_lattice: PresentationScreenLatticeDump {
                sample_position: screen_sample_position_name(
                    plan.view.screen_lattice.sample_position,
                )
                .to_string(),
                origin: screen_lattice_origin_name(plan.view.screen_lattice.origin).to_string(),
                width_source: plan.view.screen_lattice.width_source.to_string(),
                height_source: plan.view.screen_lattice.height_source.to_string(),
            },
            canonical_view_ray: PresentationViewRayDump {
                space: view_ray_space_name(plan.view.canonical_view_ray.space).to_string(),
                normalized_direction: plan.view.canonical_view_ray.normalized_direction,
                projection_input: canonical_projection_input_name(
                    plan.view.canonical_view_ray.projection_input,
                ),
            },
            allows_legacy_projection_override: plan.view.allows_legacy_projection_override,
            compatibility_projection: PresentationCompatibilityProjectionDump {
                legacy_path_active: plan.view.compatibility_projection.legacy_path_active,
                authored_world_up_override: plan
                    .view
                    .compatibility_projection
                    .authored_world_up_override,
                authored_view_scale_override: plan
                    .view
                    .compatibility_projection
                    .authored_view_scale_override,
            },
        },
        frame: PresentationFrameDump {
            outputs: plan
                .frame
                .outputs
                .iter()
                .map(|output| PresentationAttachmentDump {
                    name: output.name.to_string(),
                    kind: frame_attachment_kind_name(output.kind).to_string(),
                    element_schema: attachment_element_schema_name(&output.element_schema),
                    lifetime: attachment_lifetime_name(output.lifetime),
                    resolution: attachment_resolution_name(output.resolution).to_string(),
                    scale: attachment_resolution_scale_name(output.scale),
                    clear_policy: attachment_clear_policy_name(output.clear_policy).to_string(),
                })
                .collect(),
            primary_hit: plan.frame.primary_hit.as_ref().map(|primary_hit| {
                PresentationPrimaryHitDump {
                    attachment: primary_hit.attachment.to_string(),
                    record: primary_hit.record.to_string(),
                    fields: primary_hit.fields.iter().map(ToString::to_string).collect(),
                    depth_semantics: depth_semantics_name(primary_hit.depth_semantics).to_string(),
                    sample_identity: primary_hit.sample_identity.to_string(),
                }
            }),
            temporal_reuse: plan
                .frame
                .temporal
                .as_ref()
                .map(|temporal| temporal_reuse_name(temporal.reuse).to_string()),
            temporal_change_class: plan.frame.temporal.as_ref().map(|temporal| {
                presentation_temporal_change_class_name(temporal.change_class).to_string()
            }),
            quality: PresentationQualityDump {
                tier: wrela::presentation_plan::quality_tier_name(plan.frame.quality.tier)
                    .to_string(),
                target_fps: plan.frame.quality.target_fps,
                internal_resolution_scale: plan.frame.quality.internal_resolution_scale,
                allow_dynamic_resolution: plan.frame.quality.allow_dynamic_resolution,
                primary_max_steps: plan.frame.quality.primary_max_steps,
                allow_radiance: plan.frame.quality.allow_radiance,
                allow_media: plan.frame.quality.allow_media,
                temporal_mode: temporal_reuse_name(plan.frame.quality.temporal_mode).to_string(),
                allow_half_res_participants: plan.frame.quality.allow_half_res_participants,
                allow_hit_compaction: plan.frame.quality.allow_hit_compaction,
                degradation_order: plan
                    .frame
                    .quality
                    .degradation_order
                    .iter()
                    .map(|step| {
                        wrela::presentation_plan::quality_degradation_step_name(*step).to_string()
                    })
                    .collect(),
            },
            lighting: PresentationLightingDump {
                key_light: presentation_lighting_input_dump(&plan.frame.lighting.key_light),
                fill_direction: presentation_lighting_input_dump(
                    &plan.frame.lighting.fill_direction,
                ),
                fill_strength: presentation_lighting_input_dump(&plan.frame.lighting.fill_strength),
                ambient_color: presentation_lighting_input_dump(&plan.frame.lighting.ambient_color),
                allows_legacy_plural_lights_metadata: plan
                    .frame
                    .lighting
                    .allows_legacy_plural_lights_metadata,
            },
            observability: contract_observability_names(&plan.frame.observability),
        },
        passes: plan
            .passes
            .iter()
            .map(|pass| PresentationPassDump {
                id: pass.id.to_string(),
                kind: presentation_pass_kind_name(&pass.kind),
                screen_samples: presentation_screen_sample_pass_dump(&pass.kind),
                consumes: pass.consumes.iter().map(ToString::to_string).collect(),
                materializes: pass.materializes.iter().map(ToString::to_string).collect(),
                binding: pass
                    .binding
                    .as_ref()
                    .map(|binding| binding.as_str().to_string()),
                query_dependencies: pass
                    .query_dependencies
                    .iter()
                    .map(|contract_id| presentation_query_dependency_dump(*contract_id))
                    .collect(),
                future_acceleration_hooks: pass
                    .future_acceleration_hooks
                    .iter()
                    .map(|hook| acceleration_hook_name(*hook).to_string())
                    .collect(),
                observability: pass_observability_names(&pass.observability),
            })
            .collect(),
        frame_artifacts: plan
            .frame_artifacts
            .iter()
            .map(|artifact| PresentationFrameArtifactDump {
                id: artifact.id.to_string(),
                attachment: artifact.attachment.to_string(),
                producer_pass: artifact.producer_pass.to_string(),
                materialized: artifact.materialized,
            })
            .collect(),
        semantic_artifacts: plan
            .semantic_artifact_contracts()
            .into_iter()
            .map(observer_semantic_artifact_dump)
            .collect(),
        artifact_uses: plan
            .artifact_uses()
            .into_iter()
            .map(observer_artifact_use_dump)
            .collect(),
        bindings: plan
            .bindings
            .iter()
            .map(|binding| PresentationBindingDump {
                id: binding.id.as_str().to_string(),
                pass_kind: presentation_pass_kind_name(&binding.pass_kind),
                recipe: presentation_recipe_name(binding.recipe).to_string(),
                default_backend: dispatch_backend_name(binding.default_backend).to_string(),
                execution: presentation_binding_execution_name(binding).to_string(),
            })
            .collect(),
        observer_projection: query_program_debug::observer_projection_for_presentation_plan(plan),
        normalized_projection: query_program_debug::projection_for_presentation_plan(plan),
        validation,
    }
}

pub(crate) fn print_presentation_plan_human(dump: &PresentationPlanDump) {
    println!("presentation plan schema v{}", dump.schema_version);
    println!("entry: {}", dump.entry_path);
    if dump.plans.is_empty() {
        println!("plans: none");
        return;
    }
    for plan in &dump.plans {
        println!("plan {}", plan.name);
        println!(
            "  view: canonical_projection={} input={} compatibility_legacy_path={} authored_world_up_override={} authored_view_scale_override={}",
            plan.view.canonical_projection,
            plan.view.canonical_projection_input,
            plan.view.compatibility_projection.legacy_path_active,
            plan.view
                .compatibility_projection
                .authored_world_up_override,
            plan.view
                .compatibility_projection
                .authored_view_scale_override
        );
        println!(
            "  screen lattice: sample_position={} origin={} width={} height={}",
            plan.view.screen_lattice.sample_position,
            plan.view.screen_lattice.origin,
            plan.view.screen_lattice.width_source,
            plan.view.screen_lattice.height_source
        );
        println!(
            "  canonical view rays: space={} normalized_direction={} projection_input={}",
            plan.view.canonical_view_ray.space,
            plan.view.canonical_view_ray.normalized_direction,
            plan.view.canonical_view_ray.projection_input
        );
        let outputs = plan
            .frame
            .outputs
            .iter()
            .map(|output| {
                format!(
                    "{}({},{},{},{},{},{})",
                    output.name,
                    output.kind,
                    output.element_schema,
                    output.lifetime,
                    output.resolution,
                    output.scale,
                    output.clear_policy
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "  frame outputs: {}",
            if outputs.is_empty() { "none" } else { &outputs }
        );
        if let Some(primary_hit) = &plan.frame.primary_hit {
            println!(
                "  primary hit attachment: {} record={} depth={} sample_identity={} fields={}",
                primary_hit.attachment,
                primary_hit.record,
                primary_hit.depth_semantics,
                primary_hit.sample_identity,
                primary_hit.fields.join(",")
            );
        }
        println!(
            "  quality: tier={} target_fps={} internal_scale={:.2} dynamic_resolution={} primary_max_steps={} radiance={} media={} temporal_mode={} half_res_participants={} hit_compaction={}",
            plan.frame.quality.tier,
            plan.frame.quality.target_fps,
            plan.frame.quality.internal_resolution_scale,
            plan.frame.quality.allow_dynamic_resolution,
            plan.frame.quality.primary_max_steps,
            plan.frame.quality.allow_radiance,
            plan.frame.quality.allow_media,
            plan.frame.quality.temporal_mode,
            plan.frame.quality.allow_half_res_participants,
            plan.frame.quality.allow_hit_compaction
        );
        println!(
            "  quality degradation order: {}",
            if plan.frame.quality.degradation_order.is_empty() {
                "none".to_string()
            } else {
                plan.frame.quality.degradation_order.join(", ")
            }
        );
        println!(
            "  lighting: key_light={} fill_direction={} fill_strength={} ambient_color={} legacy_plural_lights={}",
            format_lighting_input_dump(&plan.frame.lighting.key_light),
            format_lighting_input_dump(&plan.frame.lighting.fill_direction),
            format_lighting_input_dump(&plan.frame.lighting.fill_strength),
            format_lighting_input_dump(&plan.frame.lighting.ambient_color),
            plan.frame.lighting.allows_legacy_plural_lights_metadata
        );
        println!("  passes:");
        for pass in &plan.passes {
            println!(
                "    {} kind={} binding={}",
                pass.id,
                pass.kind,
                pass.binding.as_deref().unwrap_or("none")
            );
            let queries = pass
                .query_dependencies
                .iter()
                .map(|query| {
                    let evidence = query
                        .evidence
                        .as_ref()
                        .map(|evidence| {
                            let path = if evidence.refinement_path.is_empty() {
                                "none".to_string()
                            } else {
                                evidence.refinement_path.join(" -> ")
                            };
                            format!(
                                " [evidence={} scope={} distance={} support={} lower_bound={} analytic={} path={}]",
                                evidence.origin,
                                evidence.scope,
                                evidence.distance_semantics,
                                evidence.support_class,
                                evidence.support_lower_bound_pruning,
                                evidence.analytic_intersection,
                                path
                            )
                        })
                        .unwrap_or_default();
                    let solver = query
                        .solver_diagnostics
                        .as_ref()
                        .map(|solver| {
                            format!(" [solver={} fallback={}]", solver.plan_id, solver.fallback)
                        })
                        .unwrap_or_default();
                    format!("{}{}{}", query.contract_id, evidence, solver)
                })
                .collect::<Vec<_>>()
                .join(", ");
            println!(
                "      query dependencies: {}",
                if queries.is_empty() { "none" } else { &queries }
            );
            println!(
                "      materializes: {}",
                if pass.materializes.is_empty() {
                    "none".to_string()
                } else {
                    pass.materializes.join(", ")
                }
            );
            if let Some(screen_samples) = &pass.screen_samples {
                println!(
                    "      screen samples: viewport={}x{} samples_per_pixel={} jitter={} item_count={} record={}",
                    screen_samples.viewport_width_source,
                    screen_samples.viewport_height_source,
                    screen_samples.samples_per_pixel,
                    screen_samples.jitter_source,
                    screen_samples.item_count_expression,
                    screen_samples.output_item_record
                );
            }
            println!(
                "      future acceleration hooks: {}",
                if pass.future_acceleration_hooks.is_empty() {
                    "none".to_string()
                } else {
                    pass.future_acceleration_hooks.join(", ")
                }
            );
        }
        println!("  bindings:");
        for binding in &plan.bindings {
            println!(
                "    {} recipe={} backend={} execution={}",
                binding.id, binding.recipe, binding.default_backend, binding.execution
            );
        }
        println!("  semantic artifacts:");
        for artifact in &plan.semantic_artifacts {
            println!(
                "    {} kind={} snapshot_relation={} producer={} consumer={} schema={} acceleration={} validity={}",
                artifact.id,
                artifact.kind,
                artifact.snapshot_relation,
                artifact.producer,
                artifact.consumer,
                artifact.logical_schema,
                artifact
                    .acceleration_kind
                    .as_ref()
                    .map(|kind| format!(
                        "{}:{}:{}@{}",
                        kind,
                        artifact
                            .acceleration_observer
                            .as_deref()
                            .unwrap_or("unknown"),
                        artifact
                            .acceleration_residency
                            .as_deref()
                            .unwrap_or("unknown"),
                        artifact
                            .acceleration_usage_site
                            .as_deref()
                            .unwrap_or("unknown")
                    ))
                    .unwrap_or_else(|| "none".to_string()),
                artifact.validity
            );
        }
        println!("  artifact uses:");
        for use_record in &plan.artifact_uses {
            println!(
                "    actor={} artifact={} kind={} source={} validity={}",
                use_record.actor,
                use_record.artifact_id,
                use_record.kind,
                use_record.source,
                use_record.required_validity.as_deref().unwrap_or("none")
            );
        }
        println!("  validation: {}", plan.validation.status);
        for error in &plan.validation.errors {
            println!("    {}", error);
        }
        print_observer_projection_human(&plan.observer_projection);
        println!(
            "  normalized projection (compat): family={} mode={} passes={} queries={} artifacts={}",
            plan.normalized_projection.family,
            plan.normalized_projection.execution_mode,
            if plan.normalized_projection.pass_kinds.is_empty() {
                "none".to_string()
            } else {
                plan.normalized_projection.pass_kinds.join(", ")
            },
            if plan.normalized_projection.query_contracts.is_empty() {
                "none".to_string()
            } else {
                plan.normalized_projection.query_contracts.join(", ")
            },
            if plan.normalized_projection.frame_artifacts.is_empty() {
                "none".to_string()
            } else {
                plan.normalized_projection.frame_artifacts.join(", ")
            }
        );
    }
}

pub(crate) fn canonical_projection_input_name(
    input: wrela::presentation_contract::CanonicalProjectionInput,
) -> String {
    match input {
        wrela::presentation_contract::CanonicalProjectionInput::CameraVerticalFovDegrees => {
            "Camera.vertical_fov_degrees".to_string()
        }
    }
}

pub(crate) fn screen_sample_position_name(
    position: wrela::presentation_contract::ScreenLatticeSamplePosition,
) -> &'static str {
    match position {
        wrela::presentation_contract::ScreenLatticeSamplePosition::PixelCenter => "PixelCenter",
    }
}

pub(crate) fn screen_lattice_origin_name(
    origin: wrela::presentation_contract::ScreenLatticeOrigin,
) -> &'static str {
    match origin {
        wrela::presentation_contract::ScreenLatticeOrigin::TopLeft => "TopLeft",
    }
}

pub(crate) fn view_ray_space_name(
    space: wrela::presentation_contract::CanonicalViewRaySpace,
) -> &'static str {
    match space {
        wrela::presentation_contract::CanonicalViewRaySpace::World => "World",
    }
}

pub(crate) fn depth_semantics_name(
    semantics: wrela::presentation_contract::DepthAttachmentSemantics,
) -> &'static str {
    match semantics {
        wrela::presentation_contract::DepthAttachmentSemantics::RayParameterDistance => {
            "RayParameterDistance"
        }
    }
}

pub(crate) fn frame_attachment_kind_name(
    kind: wrela::presentation_contract::FrameAttachmentKind,
) -> &'static str {
    match kind {
        wrela::presentation_contract::FrameAttachmentKind::PrimaryHit => "PrimaryHit",
        wrela::presentation_contract::FrameAttachmentKind::Depth => "Depth",
        wrela::presentation_contract::FrameAttachmentKind::WorldNormal => "WorldNormal",
        wrela::presentation_contract::FrameAttachmentKind::Surface => "Surface",
        wrela::presentation_contract::FrameAttachmentKind::Radiance => "Radiance",
        wrela::presentation_contract::FrameAttachmentKind::Medium => "Medium",
        wrela::presentation_contract::FrameAttachmentKind::Motion => "Motion",
        wrela::presentation_contract::FrameAttachmentKind::Color => "Color",
    }
}

pub(crate) fn attachment_lifetime_name(
    lifetime: wrela::presentation_contract::AttachmentLifetime,
) -> String {
    match lifetime {
        wrela::presentation_contract::AttachmentLifetime::Transient => "Transient".to_string(),
        wrela::presentation_contract::AttachmentLifetime::Exported => "Exported".to_string(),
        wrela::presentation_contract::AttachmentLifetime::HistorySlot(slot) => {
            format!("HistorySlot({slot})")
        }
    }
}

pub(crate) fn attachment_element_schema_name(
    schema: &wrela::presentation_contract::AttachmentElementSchema,
) -> String {
    match schema {
        wrela::presentation_contract::AttachmentElementSchema::NamedRecord(name) => {
            name.to_string()
        }
        wrela::presentation_contract::AttachmentElementSchema::ScalarF32 => "f32".to_string(),
        wrela::presentation_contract::AttachmentElementSchema::Vec2F32 => "vec2<f32>".to_string(),
        wrela::presentation_contract::AttachmentElementSchema::Vec3F32 => "vec3<f32>".to_string(),
        wrela::presentation_contract::AttachmentElementSchema::Vec4F32 => "vec4<f32>".to_string(),
    }
}

pub(crate) fn attachment_resolution_name(
    resolution: wrela::presentation_contract::AttachmentResolutionClass,
) -> &'static str {
    match resolution {
        wrela::presentation_contract::AttachmentResolutionClass::Viewport => "Viewport",
        wrela::presentation_contract::AttachmentResolutionClass::HalfViewport => "HalfViewport",
        wrela::presentation_contract::AttachmentResolutionClass::QuarterViewport => {
            "QuarterViewport"
        }
    }
}

pub(crate) fn attachment_resolution_scale_name(
    scale: wrela::presentation_contract::AttachmentResolutionScale,
) -> String {
    format!("{}x{}", scale.divisor_x, scale.divisor_y)
}

pub(crate) fn attachment_clear_policy_name(
    clear_policy: wrela::presentation_contract::AttachmentClearPolicy,
) -> &'static str {
    match clear_policy {
        wrela::presentation_contract::AttachmentClearPolicy::Zero => "Zero",
        wrela::presentation_contract::AttachmentClearPolicy::SemanticDefault => "SemanticDefault",
        wrela::presentation_contract::AttachmentClearPolicy::PreservePrevious => "PreservePrevious",
    }
}

pub(crate) fn temporal_reuse_name(
    reuse: wrela::presentation_contract::TemporalReuseMode,
) -> &'static str {
    match reuse {
        wrela::presentation_contract::TemporalReuseMode::Disabled => "Disabled",
        wrela::presentation_contract::TemporalReuseMode::ReprojectColor => "ReprojectColor",
        wrela::presentation_contract::TemporalReuseMode::ReprojectColorAndMotion => {
            "ReprojectColorAndMotion"
        }
    }
}

pub(crate) fn contract_observability_names(
    observability: &wrela::presentation_contract::PresentationObservabilityProfile,
) -> Vec<String> {
    let mut names = Vec::new();
    if observability.pass_graph {
        names.push("pass_graph".to_string());
    }
    if observability.materialized_intermediates {
        names.push("materialized_intermediates".to_string());
    }
    if observability.query_dependencies {
        names.push("query_dependencies".to_string());
    }
    if observability.backend_dispatch {
        names.push("backend_dispatch".to_string());
    }
    if observability.future_acceleration_hooks {
        names.push("future_acceleration_hooks".to_string());
    }
    names
}

pub(crate) fn pass_observability_names(
    observability: &wrela::presentation_plan::PresentationObservability,
) -> Vec<String> {
    let mut names = Vec::new();
    if observability.pass_graph {
        names.push("pass_graph".to_string());
    }
    if observability.materialized_intermediates {
        names.push("materialized_intermediates".to_string());
    }
    if observability.query_dependencies {
        names.push("query_dependencies".to_string());
    }
    if observability.backend_dispatch {
        names.push("backend_dispatch".to_string());
    }
    if observability.future_acceleration_hooks {
        names.push("future_acceleration_hooks".to_string());
    }
    names
}

pub(crate) fn presentation_lighting_input_dump(
    contract: &wrela::presentation_contract::LightingInputContract,
) -> PresentationLightingInputDump {
    PresentationLightingInputDump {
        binding: contract.binding.to_string(),
        element_schema: attachment_element_schema_name(&contract.element_schema),
        source: lighting_input_source_name(contract.source).to_string(),
        temporary_compatibility_alias: contract.temporary_compatibility_alias,
    }
}

pub(crate) fn format_lighting_input_dump(contract: &PresentationLightingInputDump) -> String {
    format!(
        "{}:{}:{}:compat_alias={}",
        contract.binding,
        contract.element_schema,
        contract.source,
        contract.temporary_compatibility_alias
    )
}

pub(crate) fn lighting_input_source_name(
    source: wrela::presentation_contract::LightingInputBindingSource,
) -> &'static str {
    match source {
        wrela::presentation_contract::LightingInputBindingSource::AuthoredMetadata => {
            "AuthoredMetadata"
        }
        wrela::presentation_contract::LightingInputBindingSource::DefaultCompatibilityRecipe => {
            "DefaultCompatibilityRecipe"
        }
    }
}

pub(crate) fn presentation_pass_kind_name(
    kind: &wrela::presentation_plan::PresentationPassKind,
) -> String {
    match kind {
        wrela::presentation_plan::PresentationPassKind::GenerateScreenSamples { .. } => {
            "GenerateScreenSamples".to_string()
        }
        wrela::presentation_plan::PresentationPassKind::PrimaryVisibility { contract } => {
            format!("PrimaryVisibility({})", contract.query_contract.as_str())
        }
        wrela::presentation_plan::PresentationPassKind::SurfaceResolve { contract } => {
            format!("SurfaceResolve({})", contract.query_contract.as_str())
        }
        wrela::presentation_plan::PresentationPassKind::ParticipantsResolve { contract } => {
            format!(
                "ParticipantsResolve(radiance={},medium={})",
                contract
                    .radiance_query_contract
                    .map(|contract| contract.as_str().to_string())
                    .unwrap_or_else(|| "disabled".to_string()),
                contract
                    .medium_query_contract
                    .map(|contract| contract.as_str().to_string())
                    .unwrap_or_else(|| "disabled".to_string())
            )
        }
        wrela::presentation_plan::PresentationPassKind::ShadePrimary { contract } => {
            format!("ShadePrimary({})", contract.output_attachment)
        }
        wrela::presentation_plan::PresentationPassKind::CompositeColor { contract } => {
            format!(
                "CompositeColor({}->{})",
                contract.input_attachment, contract.output_attachment
            )
        }
        wrela::presentation_plan::PresentationPassKind::MotionResolve { contract } => {
            format!(
                "MotionResolve({}->{})",
                contract.primary_hit_attachment, contract.output_attachment
            )
        }
        wrela::presentation_plan::PresentationPassKind::TemporalResolve { contract } => {
            format!(
                "TemporalResolve({}->{})",
                contract.input_attachment, contract.output_attachment
            )
        }
        wrela::presentation_plan::PresentationPassKind::WorldBatchQuery { contract_id } => {
            format!("WorldBatchQuery({})", contract_id.as_str())
        }
        wrela::presentation_plan::PresentationPassKind::KernelDispatch => {
            "KernelDispatch".to_string()
        }
        wrela::presentation_plan::PresentationPassKind::ExportAttachment { attachment } => {
            format!("ExportAttachment({attachment})")
        }
    }
}

pub(crate) fn presentation_screen_sample_pass_dump(
    kind: &wrela::presentation_plan::PresentationPassKind,
) -> Option<PresentationScreenSamplePassDump> {
    match kind {
        wrela::presentation_plan::PresentationPassKind::GenerateScreenSamples { contract } => {
            Some(PresentationScreenSamplePassDump {
                viewport_width_source: contract.viewport_width_source.to_string(),
                viewport_height_source: contract.viewport_height_source.to_string(),
                samples_per_pixel: contract.samples_per_pixel,
                jitter_source: contract.jitter_source.to_string(),
                item_count_expression: contract.item_count_expression.to_string(),
                output_item_record: contract.output_item_record.to_string(),
            })
        }
        _ => None,
    }
}

pub(crate) fn collision_pass_kind_name(kind: &wrela::collision_plan::CollisionPassKind) -> String {
    match kind {
        wrela::collision_plan::CollisionPassKind::GatherCandidates { .. } => {
            "gather_candidates".to_string()
        }
        wrela::collision_plan::CollisionPassKind::BuildBroadphaseCandidates { .. } => {
            "build_broadphase_candidates".to_string()
        }
        wrela::collision_plan::CollisionPassKind::EvaluatePointOccupancy { .. } => {
            "evaluate_point_occupancy".to_string()
        }
        wrela::collision_plan::CollisionPassKind::CastRayFirstHit { .. } => {
            "cast_ray_first_hit".to_string()
        }
        wrela::collision_plan::CollisionPassKind::ResolveSphereOverlap { .. } => {
            "resolve_sphere_overlap".to_string()
        }
        wrela::collision_plan::CollisionPassKind::SweepSphereFirstContact { .. } => {
            "sweep_sphere_first_contact".to_string()
        }
        wrela::collision_plan::CollisionPassKind::ResolveSphereTimeOfImpact { .. } => {
            "resolve_sphere_time_of_impact".to_string()
        }
        wrela::collision_plan::CollisionPassKind::MaterializeOutput { .. } => {
            "materialize_output".to_string()
        }
    }
}

pub(crate) fn presentation_recipe_name(
    recipe: wrela::presentation_binding::PresentationPassRecipeKind,
) -> &'static str {
    match recipe {
        wrela::presentation_binding::PresentationPassRecipeKind::GenerateScreenSamples => {
            "GenerateScreenSamples"
        }
        wrela::presentation_binding::PresentationPassRecipeKind::PrimaryVisibility => {
            "PrimaryVisibility"
        }
        wrela::presentation_binding::PresentationPassRecipeKind::SurfaceResolve => "SurfaceResolve",
        wrela::presentation_binding::PresentationPassRecipeKind::ParticipantsResolve => {
            "ParticipantsResolve"
        }
        wrela::presentation_binding::PresentationPassRecipeKind::ShadePrimary => "ShadePrimary",
        wrela::presentation_binding::PresentationPassRecipeKind::CompositeColor => "CompositeColor",
        wrela::presentation_binding::PresentationPassRecipeKind::MotionResolve => "MotionResolve",
        wrela::presentation_binding::PresentationPassRecipeKind::TemporalResolve => {
            "TemporalResolve"
        }
        wrela::presentation_binding::PresentationPassRecipeKind::WorldBatchQuery => {
            "WorldBatchQuery"
        }
        wrela::presentation_binding::PresentationPassRecipeKind::KernelDispatch => "KernelDispatch",
        wrela::presentation_binding::PresentationPassRecipeKind::ExportAttachment => {
            "ExportAttachment"
        }
    }
}

pub(crate) fn presentation_binding_execution_name(
    binding: &wrela::presentation_binding::PresentationBindingSummary,
) -> &'static str {
    match binding.recipe {
        wrela::presentation_binding::PresentationPassRecipeKind::GenerateScreenSamples => {
            "screen_sample_generation"
        }
        wrela::presentation_binding::PresentationPassRecipeKind::PrimaryVisibility => {
            "primary_visibility"
        }
        wrela::presentation_binding::PresentationPassRecipeKind::SurfaceResolve => {
            "surface_resolve"
        }
        wrela::presentation_binding::PresentationPassRecipeKind::ParticipantsResolve => {
            "participants_resolve"
        }
        wrela::presentation_binding::PresentationPassRecipeKind::ShadePrimary => "shade_primary",
        wrela::presentation_binding::PresentationPassRecipeKind::CompositeColor => {
            "composite_color"
        }
        wrela::presentation_binding::PresentationPassRecipeKind::MotionResolve => "motion_resolve",
        wrela::presentation_binding::PresentationPassRecipeKind::TemporalResolve => {
            "temporal_resolve"
        }
        wrela::presentation_binding::PresentationPassRecipeKind::WorldBatchQuery => {
            "world_batch_query"
        }
        wrela::presentation_binding::PresentationPassRecipeKind::KernelDispatch => {
            "kernel_dispatch"
        }
        wrela::presentation_binding::PresentationPassRecipeKind::ExportAttachment => {
            "attachment_export"
        }
    }
}

pub(crate) fn dispatch_backend_name(backend: wrela::query_plan::DispatchBackend) -> &'static str {
    match backend {
        wrela::query_plan::DispatchBackend::Cpu => "cpu",
        wrela::query_plan::DispatchBackend::VirtualGpu => "virtual_gpu",
        wrela::query_plan::DispatchBackend::Wgsl => "wgsl",
        wrela::query_plan::DispatchBackend::Auto => "auto",
    }
}

pub(crate) fn acceleration_hook_name(
    hook: wrela::presentation_plan::PresentationAccelerationHook,
) -> &'static str {
    match hook {
        wrela::presentation_plan::PresentationAccelerationHook::ScreenLattice => "ScreenLattice",
        wrela::presentation_plan::PresentationAccelerationHook::WorldBatch => "WorldBatch",
        wrela::presentation_plan::PresentationAccelerationHook::SemanticSupport => {
            "SemanticSupport"
        }
        wrela::presentation_plan::PresentationAccelerationHook::TemporalHistory => {
            "TemporalHistory"
        }
    }
}

pub(crate) fn distance_semantics_name(
    semantics: wrela::scene_ir::DistanceSemantics,
) -> &'static str {
    match semantics {
        wrela::scene_ir::DistanceSemantics::ExactSignedDistance => "exact-signed-distance",
        wrela::scene_ir::DistanceSemantics::ConservativeLowerBound => "conservative-lower-bound",
        wrela::scene_ir::DistanceSemantics::UnknownOpaque => "unknown-opaque",
    }
}

pub(crate) fn support_class_name(class: wrela::scene_ir::SupportClass) -> &'static str {
    match class {
        wrela::scene_ir::SupportClass::Unknown => "unknown",
        wrela::scene_ir::SupportClass::Bounded => "bounded",
        wrela::scene_ir::SupportClass::Periodic => "periodic",
        wrela::scene_ir::SupportClass::Unbounded => "unbounded",
    }
}

pub(crate) fn fact_availability_name(value: wrela::query_solver::FactAvailability) -> &'static str {
    match value {
        wrela::query_solver::FactAvailability::Available => "available",
        wrela::query_solver::FactAvailability::Unavailable => "unavailable",
        wrela::query_solver::FactAvailability::Unknown => "unknown",
    }
}

pub(crate) fn lipschitz_status_name(value: wrela::query_solver::LipschitzStatus) -> &'static str {
    match value {
        wrela::query_solver::LipschitzStatus::ExactKnown => "exact-known",
        wrela::query_solver::LipschitzStatus::ConservativeKnown => "conservative-known",
        wrela::query_solver::LipschitzStatus::Unknown => "unknown",
        wrela::query_solver::LipschitzStatus::Unavailable => "unavailable",
    }
}

pub(crate) fn analytic_status_name(
    value: wrela::query_solver::AnalyticIntersectionStatus,
) -> &'static str {
    match value {
        wrela::query_solver::AnalyticIntersectionStatus::Available => "available",
        wrela::query_solver::AnalyticIntersectionStatus::CandidateOnly => "candidate-only",
        wrela::query_solver::AnalyticIntersectionStatus::Unavailable => "unavailable",
        wrela::query_solver::AnalyticIntersectionStatus::Unknown => "unknown",
    }
}

pub(crate) fn temporal_stability_name(
    value: wrela::query_solver::TemporalStability,
) -> &'static str {
    match value {
        wrela::query_solver::TemporalStability::CompileInvariant => "compile-invariant",
        wrela::query_solver::TemporalStability::TransitionCompatible => "transition-compatible",
        wrela::query_solver::TemporalStability::SnapshotLocal => "snapshot-local",
        wrela::query_solver::TemporalStability::ArtifactBound => "artifact-bound",
        wrela::query_solver::TemporalStability::Unknown => "unknown",
    }
}

pub(crate) fn presentation_temporal_change_class_name(
    value: wrela::presentation_contract::TemporalChangeClass,
) -> &'static str {
    match value {
        wrela::presentation_contract::TemporalChangeClass::Stable => "stable",
        wrela::presentation_contract::TemporalChangeClass::CameraMotion => "camera-motion",
        wrela::presentation_contract::TemporalChangeClass::ViewportShift => "viewport-shift",
        wrela::presentation_contract::TemporalChangeClass::TopologyShift => "topology-shift",
        wrela::presentation_contract::TemporalChangeClass::IdentityShift => "identity-shift",
        wrela::presentation_contract::TemporalChangeClass::HistoryReset => "history-reset",
        wrela::presentation_contract::TemporalChangeClass::Unknown => "unknown",
    }
}

pub(crate) fn semantic_temporal_change_class_name(
    value: wrela::semantic_evidence::TemporalChangeClass,
) -> &'static str {
    match value {
        wrela::semantic_evidence::TemporalChangeClass::Stable => "stable",
        wrela::semantic_evidence::TemporalChangeClass::CameraMotion => "camera-motion",
        wrela::semantic_evidence::TemporalChangeClass::ViewportShift => "viewport-shift",
        wrela::semantic_evidence::TemporalChangeClass::TopologyShift => "topology-shift",
        wrela::semantic_evidence::TemporalChangeClass::IdentityShift => "identity-shift",
        wrela::semantic_evidence::TemporalChangeClass::HistoryReset => "history-reset",
        wrela::semantic_evidence::TemporalChangeClass::Unknown => "unknown",
    }
}

pub(crate) fn presentation_refinement_path_dump(
    steps: &[wrela::query_plan::SemanticEvidenceRefinementStep],
) -> Vec<String> {
    steps
        .iter()
        .map(|step| {
            let name = wrela::query_plan::semantic_evidence_refinement_step_name(step);
            if step.detail.is_empty() {
                name.to_string()
            } else {
                format!("{}({})", name, step.detail)
            }
        })
        .collect()
}

pub(crate) fn presentation_evidence_dump_from_summary(
    summary: &wrela::query_plan::SemanticEvidenceSummary,
) -> PresentationEvidenceDump {
    PresentationEvidenceDump {
        subject: summary.subject.to_string(),
        origin: wrela::query_plan::semantic_evidence_origin_name(summary.origin).to_string(),
        scope: wrela::query_plan::semantic_evidence_scope_name(summary.scope).to_string(),
        refinement_path: presentation_refinement_path_dump(&summary.refinement_path),
        distance_refinement_path: presentation_refinement_path_dump(
            &summary.distance.refinement_path,
        ),
        support_refinement_path: presentation_refinement_path_dump(
            &summary.support.refinement_path,
        ),
        differential_refinement_path: presentation_refinement_path_dump(
            &summary.differential.refinement_path,
        ),
        identity_refinement_path: presentation_refinement_path_dump(
            &summary.identity.refinement_path,
        ),
        temporal_refinement_path: presentation_refinement_path_dump(
            &summary.temporal.refinement_path,
        ),
        distance_semantics: distance_semantics_name(summary.distance.semantics).to_string(),
        support_class: support_class_name(summary.support.support_class).to_string(),
        support_lower_bound_pruning: fact_availability_name(summary.support.lower_bound_pruning)
            .to_string(),
        support_conservative_bounds: fact_availability_name(summary.support.conservative_bounds)
            .to_string(),
        lipschitz: lipschitz_status_name(summary.distance.lipschitz).to_string(),
        analytic_intersection: analytic_status_name(summary.distance.analytic_intersection)
            .to_string(),
        derivative: fact_availability_name(summary.differential.derivative).to_string(),
        stable_feature_id: summary.identity.stable_feature_id,
        stable_instance_id: summary.identity.stable_instance_id,
        stable_repeat_id: summary.identity.stable_repeat_id,
        temporal_stability: temporal_stability_name(summary.temporal.stability).to_string(),
        temporal_change_class: semantic_temporal_change_class_name(summary.temporal.change_class)
            .to_string(),
        temporal_stationary: fact_availability_name(summary.temporal.stationary).to_string(),
        temporal_rigid_over_interval: fact_availability_name(summary.temporal.rigid_over_interval)
            .to_string(),
        temporal_topology_stable: fact_availability_name(summary.temporal.topology_stable)
            .to_string(),
        temporal_bounded_velocity: fact_availability_name(summary.temporal.bounded_velocity)
            .to_string(),
    }
}

pub(crate) fn presentation_solver_dump(
    summary: &wrela::query_solver::RaySolverDiagnosticSummary,
) -> PresentationRaySolverDump {
    PresentationRaySolverDump {
        plan_id: summary.plan_id.to_string(),
        subject: summary.subject.to_string(),
        methods: summary
            .methods
            .iter()
            .map(|method| wrela::query_solver::ray_solver_method_name(*method).to_string())
            .collect(),
        mixed_selections: summary
            .mixed_selections
            .iter()
            .map(presentation_ray_solver_selection_dump)
            .collect(),
        artifact_reuse_intents: summary
            .artifact_reuse_intents
            .iter()
            .map(presentation_ray_solver_artifact_reuse_intent_dump)
            .collect(),
        continuation_intents: summary
            .continuation_intents
            .iter()
            .map(presentation_ray_solver_continuation_intent_dump)
            .collect(),
        fallback: wrela::query_solver::ray_solver_fallback_name(summary.fallback).to_string(),
        unavailable_facts: summary
            .unavailable_facts
            .iter()
            .map(|fact| fact.to_string())
            .collect(),
    }
}

pub(crate) fn presentation_ray_solver_selection_dump(
    selection: &wrela::query_solver::RaySolverMixedSelection,
) -> PresentationRaySolverSelectionDump {
    PresentationRaySolverSelectionDump {
        subject: selection.subject.to_string(),
        candidate_class: selection.candidate_class.to_string(),
        method: wrela::query_solver::ray_solver_method_name(selection.method).to_string(),
        required_guarantee: wrela::presentation_exec::cost::required_guarantee_class_name(
            selection.required_guarantee,
        )
        .to_string(),
        selected_method_class: wrela::presentation_exec::cost::selected_method_class_name(
            selection.selected_method_class,
        )
        .to_string(),
        evidence_policy_summary: selection.evidence_policy_summary.to_string(),
    }
}

pub(crate) fn presentation_ray_solver_artifact_reuse_intent_dump(
    intent: &wrela::query_solver::RaySolverArtifactReuseIntent,
) -> PresentationRaySolverIntentDump {
    PresentationRaySolverIntentDump {
        selection: presentation_ray_solver_selection_dump(&intent.selection),
        disposition: ray_solver_intent_disposition_name(intent.disposition).to_string(),
        reasons: intent
            .reasons
            .iter()
            .map(|reason| reason.to_string())
            .collect(),
    }
}

pub(crate) fn presentation_ray_solver_continuation_intent_dump(
    intent: &wrela::query_solver::RaySolverContinuationIntent,
) -> PresentationRaySolverIntentDump {
    PresentationRaySolverIntentDump {
        selection: presentation_ray_solver_selection_dump(&intent.selection),
        disposition: ray_solver_intent_disposition_name(intent.disposition).to_string(),
        reasons: intent
            .reasons
            .iter()
            .map(|reason| reason.to_string())
            .collect(),
    }
}

pub(crate) fn ray_solver_intent_disposition_name(
    disposition: wrela::query_solver::RaySolverIntentDisposition,
) -> &'static str {
    match disposition {
        wrela::query_solver::RaySolverIntentDisposition::Used => "used",
        wrela::query_solver::RaySolverIntentDisposition::Rejected => "rejected",
        wrela::query_solver::RaySolverIntentDisposition::Unavailable => "unavailable",
    }
}

pub(crate) fn presentation_query_dependency_metadata(
    contract_id: wrela::query_plan::QueryContractId,
) -> (
    Option<PresentationEvidenceDump>,
    Option<PresentationRaySolverDump>,
) {
    if let Ok(plan) = wrela::query_plan::BatchQueryPlan::for_contract(
        contract_id,
        wrela::query_plan::DispatchBackend::Auto,
        None,
    ) {
        return (
            Some(presentation_evidence_dump_from_summary(
                &plan.evidence_summary,
            )),
            plan.ray_solver
                .as_ref()
                .map(|solver| presentation_solver_dump(&solver.diagnostic_summary())),
        );
    }

    if let Ok(plan) = wrela::query_plan::CaptureQueryPlan::for_contract(contract_id, None) {
        return (
            Some(presentation_evidence_dump_from_summary(
                &plan.evidence_summary,
            )),
            None,
        );
    }

    if let Ok(plan) = wrela::query_plan::WorldQueryPlan::for_contract_with_backend(
        contract_id,
        wrela::query_plan::DispatchBackend::Auto,
    ) {
        return (
            Some(presentation_evidence_dump_from_summary(
                &plan.evidence_summary,
            )),
            plan.ray_solver
                .as_ref()
                .map(|solver| presentation_solver_dump(&solver.diagnostic_summary())),
        );
    }

    (None, None)
}

pub(crate) fn presentation_query_dependency_dump(
    contract_id: wrela::query_plan::QueryContractId,
) -> PresentationQueryDependencyDump {
    let descriptor = wrela::query_contract::query_contract(contract_id);
    let (evidence, solver_diagnostics) = presentation_query_dependency_metadata(contract_id);
    PresentationQueryDependencyDump {
        contract_id: contract_id.as_str().to_string(),
        family: descriptor.map(|descriptor| {
            wrela::query_contract::query_family_name(descriptor.family).to_string()
        }),
        question: descriptor.map(|descriptor| {
            wrela::query_contract::query_question_name(descriptor.question).to_string()
        }),
        surface: descriptor.map(|descriptor| {
            wrela::query_contract::query_surface_name(descriptor.surface).to_string()
        }),
        target: descriptor.map(|descriptor| {
            wrela::query_contract::query_target_name(descriptor.target).to_string()
        }),
        cardinality: descriptor.map(|descriptor| {
            wrela::query_contract::query_cardinality_name(descriptor.cardinality).to_string()
        }),
        call: descriptor.map(|descriptor| {
            format!(
                "{}.{}",
                wrela::query_contract::query_family_name(descriptor.family),
                wrela::query_contract::query_family_member_name(descriptor)
            )
        }),
        evidence,
        solver_diagnostics,
    }
}
