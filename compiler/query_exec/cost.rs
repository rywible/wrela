use super::QueryExecutionObservability;
use crate::execution_policy::{
    QueryExecutionPolicy, RayBudgetPolicy, RequiredGuaranteeClass, SelectedMethodClass,
};
use crate::kernel::{
    KernelBatchQueryPlan, KernelBatchQueryTrace, KernelCaptureQueryPlan, KernelPlanStage,
    KernelWorldQueryPlan,
};
use crate::query_plan::{
    BatchQueryKind, CandidateStrategy, CaptureKind, CaptureQueryKind, DerivedArtifact,
    DispatchBackend, PlanExecutor, PruningStrategy, QueryItemKind, SceneDomainFlag, SceneSummary,
    WorldQueryKind, batch_query_kind_for_contract_id, capture_query_kind_for_contract_id,
    world_query_kind_for_contract_id,
};
use crate::query_solver::ray_solver_method_name;
use crate::scene_ir::SupportClass;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticQueryScope {
    Capture {
        kind: CaptureQueryKind,
        capture_kind: CaptureKind,
    },
    World {
        kind: WorldQueryKind,
    },
    Batch {
        kind: BatchQueryKind,
        capture_kind: CaptureKind,
        item_count: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticCostUnit {
    CaptureCandidates,
    WorldShapes,
    BatchItems,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostFidelity {
    Exact,
    StructuralApproximation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticSceneCostContext {
    pub support_class: SupportClass,
    pub opaque_boundary: bool,
    pub identity_source_count: u32,
    pub support_node_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SemanticStageKind {
    Execute,
    CandidateGeneration,
    CandidatePruning,
    ArtifactLoad,
    ParticipantSelection,
    ItemIteration,
    DomainSelection,
    DispatchScaffolding,
    BackendSelection,
    CaptureLoad,
    HitContextAssembly,
    ResultAppend,
    RaySolver,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticCostStage {
    pub stage: SemanticStageKind,
    pub weight: u64,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SemanticCostCauseKind {
    MarchPressure,
    SupportTopology,
    CandidateTraversal,
    CaptureArtifacts,
    DomainGating,
    ParticipantAccumulation,
    IdentityLocality,
    OpaqueFallback,
    BackendDispatch,
    RaySolverFallback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticCostCause {
    pub kind: SemanticCostCauseKind,
    pub score: u64,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticCostReport {
    pub scope: SemanticQueryScope,
    pub backend: DispatchBackend,
    pub executor: PlanExecutor,
    pub item_kind: QueryItemKind,
    pub unit: SemanticCostUnit,
    pub fidelity: CostFidelity,
    pub candidate_strategy: CandidateStrategy,
    pub pruning_strategy: PruningStrategy,
    pub preserves_local_hit_context: bool,
    pub scene: Option<SemanticSceneCostContext>,
    pub artifact_labels: Vec<String>,
    pub domain_flags: Vec<SceneDomainFlag>,
    pub execution_policy: Option<QueryExecutionPolicy>,
    pub execution_degradations: Vec<String>,
    pub dominant_stages: Vec<SemanticCostStage>,
    pub causes: Vec<SemanticCostCause>,
    pub counters: QueryExecutionObservability,
}

pub(crate) fn capture_cost_report(
    backend: DispatchBackend,
    plan: &KernelCaptureQueryPlan,
    observability: &QueryExecutionObservability,
) -> SemanticCostReport {
    let context = SemanticCostContext {
        scope: SemanticQueryScope::Capture {
            kind: capture_query_kind_for_contract_id(plan.contract_id)
                .expect("capture query plan contract id must resolve"),
            capture_kind: plan.capture_kind,
        },
        backend,
        executor: plan.executor,
        item_kind: plan.candidate_contract.item_kind,
        item_count: 1,
        candidate_strategy: plan.candidate_strategy,
        pruning_strategy: plan.pruning_strategy,
        preserves_local_hit_context: plan.preserves_local_hit_context,
        participant_kind: plan
            .participant_contract
            .as_ref()
            .map(|contract| contract.kind),
        domain_flags: &[],
        derived_artifacts: &plan.derived_artifacts,
        stages: &plan.stages,
        scene: plan.scene.as_ref(),
        counters: observability,
        execution_policy: None,
        execution_backend: backend,
    };
    build_report(context)
}

pub(crate) fn world_cost_report(
    backend: DispatchBackend,
    policy: &QueryExecutionPolicy,
    plan: &KernelWorldQueryPlan,
    observability: &QueryExecutionObservability,
) -> SemanticCostReport {
    let context = SemanticCostContext {
        scope: SemanticQueryScope::World {
            kind: world_query_kind_for_contract_id(plan.contract_id)
                .expect("world query plan contract id must resolve"),
        },
        backend,
        executor: plan.executor,
        item_kind: plan.dispatch_contract.item_kind,
        item_count: 1,
        candidate_strategy: plan.candidate_strategy,
        pruning_strategy: plan.pruning_strategy,
        preserves_local_hit_context: plan.preserves_local_hit_context,
        participant_kind: plan
            .participant_contract
            .as_ref()
            .map(|contract| contract.kind),
        domain_flags: &plan.domain_flags,
        derived_artifacts: &plan.derived_artifacts,
        stages: &plan.stages,
        scene: None,
        counters: observability,
        execution_policy: Some(*policy),
        execution_backend: backend,
    };
    build_report(context)
}

pub(crate) fn batch_cost_report(
    backend: DispatchBackend,
    plan: &KernelBatchQueryPlan,
    trace: &KernelBatchQueryTrace,
    observability: &QueryExecutionObservability,
) -> SemanticCostReport {
    let item_count = trace.iterations.len() as u32;
    let context = SemanticCostContext {
        scope: SemanticQueryScope::Batch {
            kind: batch_query_kind_for_contract_id(plan.contract_id)
                .expect("batch query plan contract id must resolve"),
            capture_kind: plan.capture_kind,
            item_count,
        },
        backend,
        executor: plan.executor,
        item_kind: plan.item_kind,
        item_count,
        candidate_strategy: plan.candidate_strategy,
        pruning_strategy: plan.pruning_strategy,
        preserves_local_hit_context: plan.preserves_local_hit_context,
        participant_kind: plan
            .participant_contract
            .as_ref()
            .map(|contract| contract.kind),
        domain_flags: &plan.domain_flags,
        derived_artifacts: &plan.derived_artifacts,
        stages: &plan.stages,
        scene: plan.scene.as_ref(),
        counters: observability,
        execution_policy: None,
        execution_backend: backend,
    };
    build_report(context)
}

pub fn render_semantic_cost_report(report: &SemanticCostReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "scope={} backend={} executor={} item_kind={} candidate_strategy={} pruning_strategy={}\n",
        scope_label(&report.scope),
        backend_label(report.backend),
        executor_label(report.executor),
        item_kind_label(report.item_kind),
        candidate_strategy_label(report.candidate_strategy),
        pruning_strategy_label(report.pruning_strategy),
    ));
    out.push_str(&format!(
        "unit={} fidelity={}\n",
        cost_unit_label(report.unit),
        cost_fidelity_label(report.fidelity),
    ));
    if let Some(scene) = report.scene.as_ref() {
        out.push_str(&format!(
            "scene support_class={} opaque_boundary={} identity_sources={} support_nodes={}\n",
            support_class_label(scene.support_class),
            scene.opaque_boundary,
            scene.identity_source_count,
            scene.support_node_count,
        ));
    }
    if !report.artifact_labels.is_empty() {
        out.push_str(&format!(
            "artifacts={}\n",
            report.artifact_labels.join(", ")
        ));
    }
    if !report.domain_flags.is_empty() {
        out.push_str(&format!(
            "domain_flags={}\n",
            report
                .domain_flags
                .iter()
                .map(|flag| domain_flag_label(*flag))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if let Some(policy) = report.execution_policy.as_ref() {
        out.push_str(&format!(
            "execution_policy={} degradations={}\n",
            render_execution_policy(policy),
            if report.execution_degradations.is_empty() {
                "none".to_string()
            } else {
                report.execution_degradations.join(", ")
            }
        ));
    }
    out.push_str("dominant_stages:\n");
    for stage in &report.dominant_stages {
        out.push_str(&format!(
            "- {} weight={} {}\n",
            stage_label(stage.stage),
            stage.weight,
            stage.detail
        ));
    }
    out.push_str("causes:\n");
    for cause in &report.causes {
        out.push_str(&format!(
            "- {} score={} {}\n",
            cause_label(cause.kind),
            cause.score,
            cause.detail
        ));
    }
    let traced_items = report
        .counters
        .hit_count
        .saturating_add(report.counters.miss_count);
    let trace_steps_avg = if traced_items == 0 {
        0.0
    } else {
        report.counters.trace_steps as f32 / traced_items as f32
    };
    let solver_plan = report
        .counters
        .solver_plan_id
        .as_ref()
        .map(|id| id.as_str())
        .unwrap_or("none");
    let solver_methods = if report.counters.solver_methods.is_empty() {
        "none".to_string()
    } else {
        report
            .counters
            .solver_methods
            .iter()
            .map(|method| ray_solver_method_name(*method))
            .collect::<Vec<_>>()
            .join("|")
    };
    out.push_str(&format!(
        "counters dispatch={} dispatch_items={} workgroups={}x{}x{} screen_samples={} world_batch_items={} candidates={} candidates_before={} candidates_after={} pruned={} trace_steps={} trace_steps_avg={:.2} trace_steps_max={} hits={} misses={} field_samples={} artifacts={} opaque_fallbacks={} dense_batches={} semantic_pruned_batches={} solver_plan={} solver_methods={} solver_analytic_hits={} solver_support_rejections={} solver_interval_skips={} solver_packet_tile_rejections={} solver_newton_refinements={} solver_lipschitz_steps={} solver_adaptive_epsilon={} solver_dense_fallback_rays={} solver_generated_dense_fallback_rays={} solver_fallback_contract_dense={} solver_fallback_missing_facts={} solver_fallback_analytic_unsupported={} solver_fallback_verification_failed={} solver_fallback_unsupported_backend={} solver_certificate_failures={}",
        report.counters.dispatch_count,
        report.counters.dispatch_items,
        report.counters.dispatch_workgroups_x,
        report.counters.dispatch_workgroups_y,
        report.counters.dispatch_workgroups_z,
        report.counters.screen_sample_count,
        report.counters.world_batch_item_count,
        report.counters.candidate_count,
        report.counters.candidates_before_pruning,
        report.counters.candidates_after_pruning,
        report.counters.support_pruned_candidates,
        report.counters.trace_steps,
        trace_steps_avg,
        report.counters.trace_steps_max,
        report.counters.hit_count,
        report.counters.miss_count,
        report.counters.field_samples,
        report.counters.artifact_loads,
        report.counters.opaque_fallbacks,
        report.counters.dense_compatibility_batches,
        report.counters.semantic_pruned_batches,
        solver_plan,
        solver_methods,
        report.counters.solver_analytic_hits,
        report.counters.solver_support_rejections,
        report.counters.solver_interval_skips,
        report.counters.solver_packet_tile_rejections,
        report.counters.solver_newton_refinements,
        report.counters.solver_lipschitz_steps,
        report.counters.solver_adaptive_epsilon_uses,
        report.counters.solver_dense_fallback_rays,
        report.counters.solver_generated_dense_fallback_rays,
        report.counters.solver_fallback_contract_dense,
        report.counters.solver_fallback_missing_facts,
        report.counters.solver_fallback_analytic_unsupported,
        report.counters.solver_fallback_verification_failed,
        report.counters.solver_fallback_unsupported_backend,
        report.counters.solver_certificate_failures,
    ));
    out
}

struct SemanticCostContext<'a> {
    scope: SemanticQueryScope,
    backend: DispatchBackend,
    execution_backend: DispatchBackend,
    executor: PlanExecutor,
    item_kind: QueryItemKind,
    item_count: u32,
    candidate_strategy: CandidateStrategy,
    pruning_strategy: PruningStrategy,
    preserves_local_hit_context: bool,
    participant_kind: Option<CaptureQueryKind>,
    domain_flags: &'a [SceneDomainFlag],
    derived_artifacts: &'a [DerivedArtifact],
    stages: &'a [KernelPlanStage],
    scene: Option<&'a SceneSummary>,
    counters: &'a QueryExecutionObservability,
    execution_policy: Option<QueryExecutionPolicy>,
}

fn build_report(context: SemanticCostContext<'_>) -> SemanticCostReport {
    let artifact_labels = context
        .derived_artifacts
        .iter()
        .map(artifact_label)
        .map(str::to_string)
        .collect::<Vec<_>>();
    let unit = cost_unit_for_scope(&context.scope);
    let fidelity = cost_fidelity(context.backend);
    let scene = context.scene.map(|scene| SemanticSceneCostContext {
        support_class: scene.support_class,
        opaque_boundary: scene.opaque_boundary,
        identity_source_count: scene.identity_source_count,
        support_node_count: scene.support_node_count,
    });
    let dominant_stages = collect_dominant_stages(&context, &artifact_labels);
    let causes = collect_causes(&context, &artifact_labels, scene.as_ref());
    let execution_degradations = context
        .execution_policy
        .as_ref()
        .map(|policy| execution_degradations(policy, context.execution_backend))
        .unwrap_or_default();

    SemanticCostReport {
        scope: context.scope,
        backend: context.backend,
        executor: context.executor,
        item_kind: context.item_kind,
        unit,
        fidelity,
        candidate_strategy: context.candidate_strategy,
        pruning_strategy: context.pruning_strategy,
        preserves_local_hit_context: context.preserves_local_hit_context,
        scene,
        artifact_labels,
        domain_flags: context.domain_flags.to_vec(),
        execution_policy: context.execution_policy,
        execution_degradations,
        dominant_stages,
        causes,
        counters: context.counters.clone(),
    }
}

fn collect_dominant_stages(
    context: &SemanticCostContext<'_>,
    artifact_labels: &[String],
) -> Vec<SemanticCostStage> {
    let counters = context.counters;
    let mut stages = Vec::new();

    push_stage_if_present(
        &mut stages,
        context,
        SemanticStageKind::BackendSelection,
        has_stage(context.stages, |stage| {
            matches!(stage, KernelPlanStage::SelectBackend)
        })
        .then_some((
            u64::from(counters.dispatch_count.max(1)),
            format!(
                "shared plan selected the {} backend",
                backend_label(context.backend)
            ),
        )),
    );
    push_stage_if_present(
        &mut stages,
        context,
        SemanticStageKind::CaptureLoad,
        has_stage(context.stages, |stage| {
            matches!(stage, KernelPlanStage::LoadCapture)
        })
        .then_some((
            1,
            format!(
                "loaded {} capture inputs",
                scope_capture_label(&context.scope)
            ),
        )),
    );
    push_stage_if_present(
        &mut stages,
        context,
        SemanticStageKind::ArtifactLoad,
        has_stage(context.stages, |stage| {
            matches!(stage, KernelPlanStage::LoadDerivedArtifact { .. })
        })
        .then(|| {
            let weight = u64::from(counters.artifact_loads.max(artifact_labels.len() as u32));
            let detail = if artifact_labels.is_empty() {
                "derived artifacts stayed empty for this plan".to_string()
            } else {
                format!("loaded derived artifacts [{}]", artifact_labels.join(", "))
            };
            (weight, detail)
        }),
    );
    push_stage_if_present(
        &mut stages,
        context,
        SemanticStageKind::ItemIteration,
        has_stage(context.stages, |stage| {
            matches!(stage, KernelPlanStage::IterateItems { .. })
        })
        .then_some((
            u64::from(context.item_count.max(1)),
            format!(
                "iterated {} {} items",
                context.item_count.max(1),
                item_kind_label(context.item_kind)
            ),
        )),
    );
    push_stage_if_present(
        &mut stages,
        context,
        SemanticStageKind::CandidateGeneration,
        has_stage(context.stages, |stage| {
            matches!(stage, KernelPlanStage::GenerateCandidates { .. })
        })
        .then_some((
            u64::from(
                context
                    .counters
                    .candidate_count
                    .saturating_add(context.counters.support_pruned_candidates),
            ),
            format!(
                "{} executed candidates ({} before pruning, {} after) and {} support-pruned candidates under {}",
                context.counters.candidate_count,
                context.counters.candidates_before_pruning,
                context.counters.candidates_after_pruning,
                context.counters.support_pruned_candidates,
                candidate_strategy_label(context.candidate_strategy)
            ),
        )),
    );
    push_stage_if_present(
        &mut stages,
        context,
        SemanticStageKind::CandidatePruning,
        has_stage(context.stages, |stage| {
            matches!(stage, KernelPlanStage::PruneCandidates { .. })
        })
        .then_some((
            u64::from(context.counters.support_pruned_candidates),
            format!(
                "pruned {} candidates via {}",
                context.counters.support_pruned_candidates,
                pruning_strategy_label(context.pruning_strategy)
            ),
        )),
    );
    push_stage_if_present(
        &mut stages,
        context,
        SemanticStageKind::DomainSelection,
        has_stage(context.stages, |stage| {
            matches!(stage, KernelPlanStage::LoadDomainFlags)
        })
        .then_some((
            u64::from(context.domain_flags.len().max(1) as u32),
            format!(
                "domain gating enabled [{}]",
                context
                    .domain_flags
                    .iter()
                    .map(|flag| domain_flag_label(*flag))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )),
    );
    push_stage_if_present(
        &mut stages,
        context,
        SemanticStageKind::ParticipantSelection,
        has_stage(context.stages, |stage| {
            matches!(stage, KernelPlanStage::SelectParticipants { .. })
        })
        .then(|| {
            let kind = context.participant_kind.expect("participant stage kind");
            let weight = u64::from(context.counters.candidate_count.max(1));
            (
                weight,
                format!(
                    "{} participants selected for {} accumulation",
                    context.counters.candidate_count.max(1),
                    capture_query_kind_label(kind)
                ),
            )
        }),
    );
    push_stage_if_present(
        &mut stages,
        context,
        SemanticStageKind::Execute,
        has_stage(context.stages, |stage| {
            matches!(stage, KernelPlanStage::Execute { .. })
        })
        .then_some((
            u64::from(
                context
                    .counters
                    .trace_steps
                    .saturating_add(context.counters.field_samples)
                    .saturating_add(context.counters.candidate_count.max(1)),
            ),
            format!(
                "{} trace steps, {} field samples, {} executed candidates",
                context.counters.trace_steps,
                context.counters.field_samples,
                context.counters.candidate_count
            ),
        )),
    );
    push_stage_if_present(
        &mut stages,
        context,
        SemanticStageKind::RaySolver,
        context.counters.solver_plan_id.as_ref().map(|plan_id| {
            let weight = u64::from(
                context
                    .counters
                    .solver_dense_fallback_rays
                    .saturating_add(context.counters.solver_generated_dense_fallback_rays)
                    .saturating_add(context.counters.solver_analytic_hits)
                    .max(1),
            );
            let methods = if context.counters.solver_methods.is_empty() {
                "dense fallback".to_string()
            } else {
                context
                    .counters
                    .solver_methods
                    .iter()
                    .map(|method| ray_solver_method_name(*method))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            (
                weight,
                format!("ray solver {plan_id} used {methods} under the query contract"),
            )
        }),
    );
    push_stage_if_present(
        &mut stages,
        context,
        SemanticStageKind::HitContextAssembly,
        has_stage(context.stages, |stage| {
            matches!(stage, KernelPlanStage::AssembleHitContext)
        })
        .then_some((
            1,
            "assembled local/world hit context without losing provenance".to_string(),
        )),
    );
    push_stage_if_present(
        &mut stages,
        context,
        SemanticStageKind::ResultAppend,
        has_stage(context.stages, |stage| {
            matches!(stage, KernelPlanStage::AppendResult { .. })
        })
        .then_some((
            u64::from(context.item_count.max(1)),
            format!("appended {} result records", context.item_count.max(1)),
        )),
    );
    let dispatch_scaffolding = has_stage(context.stages, |stage| {
        matches!(
            stage,
            KernelPlanStage::BeginVirtualGpuDispatch | KernelPlanStage::EndVirtualGpuDispatch
        )
    });
    push_stage_if_present(
        &mut stages,
        context,
        SemanticStageKind::DispatchScaffolding,
        dispatch_scaffolding.then_some((
            u64::from(context.counters.dispatch_count.max(1)),
            "virtual GPU / WGSL dispatch scaffolding wrapped execution".to_string(),
        )),
    );

    stages.sort_by(|lhs, rhs| {
        rhs.weight
            .cmp(&lhs.weight)
            .then(lhs.stage.cmp(&rhs.stage))
            .then(lhs.detail.cmp(&rhs.detail))
    });
    stages
}

fn collect_causes(
    context: &SemanticCostContext<'_>,
    artifact_labels: &[String],
    scene: Option<&SemanticSceneCostContext>,
) -> Vec<SemanticCostCause> {
    let counters = context.counters;
    let mut causes = Vec::new();
    let total_candidates = counters
        .candidate_count
        .saturating_add(counters.support_pruned_candidates);

    if counters.trace_steps > 0 || counters.field_samples > 0 {
        causes.push(SemanticCostCause {
            kind: SemanticCostCauseKind::MarchPressure,
            score: u64::from(counters.trace_steps.saturating_add(counters.field_samples)),
            detail: format!(
                "{} trace steps and {} field samples dominated execution",
                counters.trace_steps, counters.field_samples
            ),
        });
    }

    if matches!(
        context.candidate_strategy,
        CandidateStrategy::SupportAcceleratedShapeTraversal
            | CandidateStrategy::ShapeBranchTraversal
            | CandidateStrategy::SurfaceHitReuse
            | CandidateStrategy::OpaqueFallback
    ) || total_candidates > 0
    {
        let prune_rate = if total_candidates == 0 {
            0.0
        } else {
            (counters.support_pruned_candidates as f32 / total_candidates as f32) * 100.0
        };
        let support_hint = if counters.support_pruned_candidates == 0
            && matches!(
                context.candidate_strategy,
                CandidateStrategy::SupportAcceleratedShapeTraversal
            )
            && counters.candidate_count > 1
        {
            "support overlap kept multiple candidates live"
        } else if counters.support_pruned_candidates > 0 {
            "coarse supports eliminated work before exact evaluation"
        } else {
            "candidate traversal stayed mostly exact"
        };
        causes.push(SemanticCostCause {
            kind: SemanticCostCauseKind::SupportTopology,
            score: u64::from(total_candidates.max(1)),
            detail: format!(
                "{}; strategy={} pruning={} executed={} pruned={} prune_rate={:.1}%",
                support_hint,
                candidate_strategy_label(context.candidate_strategy),
                pruning_strategy_label(context.pruning_strategy),
                counters.candidate_count,
                counters.support_pruned_candidates,
                prune_rate
            ),
        });
    }

    if counters.candidate_count > 0 {
        causes.push(SemanticCostCause {
            kind: SemanticCostCauseKind::CandidateTraversal,
            score: u64::from(counters.candidate_count),
            detail: format!(
                "{} candidates survived into {}",
                counters.candidate_count,
                executor_label(context.executor)
            ),
        });
    }

    if !artifact_labels.is_empty() || counters.artifact_loads > 0 {
        causes.push(SemanticCostCause {
            kind: SemanticCostCauseKind::CaptureArtifacts,
            score: u64::from(counters.artifact_loads.max(artifact_labels.len() as u32)),
            detail: format!(
                "capture specialization depended on [{}]",
                artifact_labels.join(", ")
            ),
        });
    }

    if !context.domain_flags.is_empty() {
        causes.push(SemanticCostCause {
            kind: SemanticCostCauseKind::DomainGating,
            score: context.domain_flags.len() as u64,
            detail: format!(
                "domain flags constrained work to [{}]",
                context
                    .domain_flags
                    .iter()
                    .map(|flag| domain_flag_label(*flag))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
    }

    if let Some(kind) = context.participant_kind {
        causes.push(SemanticCostCause {
            kind: SemanticCostCauseKind::ParticipantAccumulation,
            score: u64::from(total_candidates.max(1)),
            detail: format!(
                "{} participant accumulation preserved provenance-aware {} semantics",
                total_candidates.max(1),
                capture_query_kind_label(kind)
            ),
        });
    }

    let repeated_identity_sources = scene
        .map(|scene| scene.identity_source_count)
        .unwrap_or_default();
    if context.preserves_local_hit_context
        || matches!(
            context.candidate_strategy,
            CandidateStrategy::SurfaceHitReuse
        )
        || repeated_identity_sources > 1
    {
        let detail = if matches!(
            context.candidate_strategy,
            CandidateStrategy::SurfaceHitReuse
        ) {
            "surface queries reused hit-selected roots without changing local hit identity"
                .to_string()
        } else {
            format!(
                "local frames and repeated-instance identity stayed stable across {} identity sources",
                repeated_identity_sources.max(1)
            )
        };
        causes.push(SemanticCostCause {
            kind: SemanticCostCauseKind::IdentityLocality,
            score: u64::from(repeated_identity_sources.max(1)),
            detail,
        });
    }

    if counters.opaque_fallbacks > 0
        || matches!(
            context.candidate_strategy,
            CandidateStrategy::OpaqueFallback
        )
        || scene.is_some_and(|scene| scene.opaque_boundary)
    {
        causes.push(SemanticCostCause {
            kind: SemanticCostCauseKind::OpaqueFallback,
            score: u64::from(counters.opaque_fallbacks.max(1)),
            detail: format!(
                "opaque or quarantine boundaries forced {} fallback paths",
                counters.opaque_fallbacks.max(1)
            ),
        });
    }

    let solver_fallbacks = counters
        .solver_dense_fallback_rays
        .saturating_add(counters.solver_generated_dense_fallback_rays);
    if counters.solver_plan_id.is_some()
        || solver_fallbacks > 0
        || counters.solver_analytic_hits > 0
        || counters.solver_support_rejections > 0
    {
        causes.push(SemanticCostCause {
            kind: SemanticCostCauseKind::RaySolverFallback,
            score: u64::from(
                solver_fallbacks
                    .saturating_add(counters.solver_analytic_hits)
                    .saturating_add(counters.solver_support_rejections)
                    .max(1),
            ),
            detail: format!(
                "ray solver plan={} dense_fallback={} generated_dense_fallback={} analytic_hits={} support_rejections={} certificate_failures={}",
                counters
                    .solver_plan_id
                    .as_ref()
                    .map(|id| id.as_str())
                    .unwrap_or("none"),
                counters.solver_dense_fallback_rays,
                counters.solver_generated_dense_fallback_rays,
                counters.solver_analytic_hits,
                counters.solver_support_rejections,
                counters.solver_certificate_failures
            ),
        });
    }

    if !matches!(context.backend, DispatchBackend::Cpu) {
        causes.push(SemanticCostCause {
            kind: SemanticCostCauseKind::BackendDispatch,
            score: u64::from(counters.dispatch_count.max(1)),
            detail: format!(
                "{} dispatch used the shared {} lane",
                scope_label(&context.scope),
                backend_label(context.backend)
            ),
        });
    }

    causes.sort_by(|lhs, rhs| {
        rhs.score
            .cmp(&lhs.score)
            .then(lhs.kind.cmp(&rhs.kind))
            .then(lhs.detail.cmp(&rhs.detail))
    });
    causes
}

fn push_stage_if_present(
    stages: &mut Vec<SemanticCostStage>,
    _context: &SemanticCostContext<'_>,
    stage: SemanticStageKind,
    candidate: Option<(u64, String)>,
) {
    let Some((weight, detail)) = candidate else {
        return;
    };
    if weight == 0 {
        return;
    }
    stages.push(SemanticCostStage {
        stage,
        weight,
        detail,
    });
}

fn has_stage<F>(stages: &[KernelPlanStage], predicate: F) -> bool
where
    F: Fn(&KernelPlanStage) -> bool,
{
    stages.iter().any(predicate)
}

fn artifact_label(artifact: &DerivedArtifact) -> &'static str {
    match artifact {
        DerivedArtifact::SupportSummary { .. } => "support-summary",
        DerivedArtifact::CaptureCache { .. } => "capture-cache",
        DerivedArtifact::CullingTable { .. } => "culling-table",
        DerivedArtifact::OpaquePessimizationBoundary => "opaque-boundary",
    }
}

fn scope_label(scope: &SemanticQueryScope) -> String {
    match scope {
        SemanticQueryScope::Capture { kind, capture_kind } => format!(
            "capture:{}:{}",
            capture_kind_label(*capture_kind),
            capture_query_kind_label(*kind)
        ),
        SemanticQueryScope::World { kind } => format!("world:{}", world_query_kind_label(*kind)),
        SemanticQueryScope::Batch {
            kind,
            capture_kind,
            item_count,
        } => format!(
            "batch:{}:{}:{}",
            capture_kind_label(*capture_kind),
            batch_query_kind_label(*kind),
            item_count
        ),
    }
}

fn scope_capture_label(scope: &SemanticQueryScope) -> &'static str {
    match scope {
        SemanticQueryScope::Capture { capture_kind, .. } => capture_kind_label(*capture_kind),
        SemanticQueryScope::World { .. } => "region",
        SemanticQueryScope::Batch { capture_kind, .. } => capture_kind_label(*capture_kind),
    }
}

fn backend_label(backend: DispatchBackend) -> &'static str {
    match backend {
        DispatchBackend::Cpu => "cpu",
        DispatchBackend::VirtualGpu => "virtual-gpu",
        DispatchBackend::Wgsl => "wgsl",
        DispatchBackend::Auto => "auto",
    }
}

fn executor_label(executor: PlanExecutor) -> &'static str {
    match executor {
        PlanExecutor::FieldDistanceCapture => "field-distance",
        PlanExecutor::ShapeDistanceCapture => "shape-distance",
        PlanExecutor::FieldSupportSummaryCapture => "field-support-summary",
        PlanExecutor::ShapeSupportSummaryCapture => "shape-support-summary",
        PlanExecutor::FieldNormalCapture => "field-normal",
        PlanExecutor::ShapeNormalCapture => "shape-normal",
        PlanExecutor::SceneTraceCapture => "scene-trace",
        PlanExecutor::SceneSurfaceCapture => "scene-surface",
        PlanExecutor::SceneRadianceCapture => "scene-radiance",
        PlanExecutor::SceneMediumCapture => "scene-medium",
        PlanExecutor::WorldDistanceCapture => "world-distance",
        PlanExecutor::WorldNormalCapture => "world-normal",
        PlanExecutor::WorldSupportSummaryCapture => "world-support-summary",
        PlanExecutor::WorldTraceCapture => "world-trace",
        PlanExecutor::WorldSurfaceCapture => "world-surface",
        PlanExecutor::WorldRadianceCapture => "world-radiance",
        PlanExecutor::WorldMediumCapture => "world-medium",
    }
}

fn item_kind_label(item_kind: QueryItemKind) -> &'static str {
    match item_kind {
        QueryItemKind::Unit => "unit",
        QueryItemKind::PointQuery => "point",
        QueryItemKind::PointDirectionQuery => "point-direction",
        QueryItemKind::RayQuery => "ray",
        QueryItemKind::Hit3 => "hit3",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_kind_label_covers_unit_queries() {
        assert_eq!(item_kind_label(QueryItemKind::Unit), "unit");
        assert_eq!(
            item_kind_label(QueryItemKind::PointDirectionQuery),
            "point-direction"
        );
    }
}

fn candidate_strategy_label(strategy: CandidateStrategy) -> &'static str {
    match strategy {
        CandidateStrategy::DirectFieldCapture => "direct-field-capture",
        CandidateStrategy::SemanticSupportSummary => "semantic-support-summary",
        CandidateStrategy::ShapeBranchTraversal => "shape-branch-traversal",
        CandidateStrategy::SupportAcceleratedShapeTraversal => {
            "support-accelerated-shape-traversal"
        }
        CandidateStrategy::SurfaceHitReuse => "surface-hit-reuse",
        CandidateStrategy::OpaqueFallback => "opaque-fallback",
    }
}

fn pruning_strategy_label(strategy: PruningStrategy) -> &'static str {
    match strategy {
        PruningStrategy::None => "none",
        PruningStrategy::ConservativeTraversal => "conservative-traversal",
        PruningStrategy::SupportLowerBound => "support-lower-bound",
        PruningStrategy::CullingTable => "culling-table",
        PruningStrategy::OpaquePessimizationBoundary => "opaque-pessimization-boundary",
    }
}

fn capture_kind_label(capture_kind: CaptureKind) -> &'static str {
    match capture_kind {
        CaptureKind::Field => "field",
        CaptureKind::Shape => "shape",
        CaptureKind::Region => "region",
    }
}

fn cost_unit_for_scope(scope: &SemanticQueryScope) -> SemanticCostUnit {
    match scope {
        SemanticQueryScope::Capture { .. } => SemanticCostUnit::CaptureCandidates,
        SemanticQueryScope::World { .. } => SemanticCostUnit::WorldShapes,
        SemanticQueryScope::Batch { .. } => SemanticCostUnit::BatchItems,
    }
}

fn cost_fidelity(backend: DispatchBackend) -> CostFidelity {
    match backend {
        DispatchBackend::Cpu | DispatchBackend::VirtualGpu => CostFidelity::Exact,
        DispatchBackend::Wgsl | DispatchBackend::Auto => CostFidelity::StructuralApproximation,
    }
}

fn capture_query_kind_label(kind: CaptureQueryKind) -> &'static str {
    match kind {
        CaptureQueryKind::Distance => "distance",
        CaptureQueryKind::Normal => "normal",
        CaptureQueryKind::SupportSummary => "support-summary",
        CaptureQueryKind::Radiance => "radiance",
        CaptureQueryKind::Medium => "medium",
        CaptureQueryKind::Nearest => "nearest",
        CaptureQueryKind::Trace => "trace",
        CaptureQueryKind::Surface => "surface",
        CaptureQueryKind::Occluded => "occluded",
    }
}

fn world_query_kind_label(kind: WorldQueryKind) -> &'static str {
    match kind {
        WorldQueryKind::Distance => "distance",
        WorldQueryKind::Normal => "normal",
        WorldQueryKind::SupportSummary => "support-summary",
        WorldQueryKind::Radiance => "radiance",
        WorldQueryKind::Medium => "medium",
        WorldQueryKind::Nearest => "nearest",
        WorldQueryKind::Trace => "trace",
        WorldQueryKind::Surface => "surface",
        WorldQueryKind::Occluded => "occluded",
    }
}

fn batch_query_kind_label(kind: BatchQueryKind) -> &'static str {
    match kind {
        BatchQueryKind::Distance => "distance",
        BatchQueryKind::Normal => "normal",
        BatchQueryKind::Nearest => "nearest",
        BatchQueryKind::Trace => "trace",
        BatchQueryKind::Surface => "surface",
        BatchQueryKind::Occluded => "occluded",
        BatchQueryKind::Radiance => "radiance",
        BatchQueryKind::Medium => "medium",
    }
}

fn domain_flag_label(flag: SceneDomainFlag) -> &'static str {
    match flag {
        SceneDomainFlag::Material => "material",
        SceneDomainFlag::Radiance => "radiance",
        SceneDomainFlag::Media => "media",
    }
}

fn render_execution_policy(policy: &QueryExecutionPolicy) -> String {
    let ray_budget = policy
        .ray_budget
        .map(format_ray_budget)
        .unwrap_or_else(|| "none".to_string());
    format!(
        "backend_preference={} required_guarantee={} selected_method={} ray_budget={}",
        backend_label(policy.backend_preference),
        required_guarantee_class_name(policy.required_guarantee),
        selected_method_class_name(policy.selected_method),
        ray_budget
    )
}

fn execution_degradations(policy: &QueryExecutionPolicy, backend: DispatchBackend) -> Vec<String> {
    let mut degradations = Vec::new();
    if !matches!(backend, DispatchBackend::Cpu) {
        degradations.push(format!(
            "backend={} runs without the CPU legality oracle",
            backend_label(backend)
        ));
    }
    if !matches!(policy.backend_preference, DispatchBackend::Auto)
        && policy.backend_preference != backend
    {
        degradations.push(format!(
            "backend_preference={} resolved_backend={}",
            backend_label(policy.backend_preference),
            backend_label(backend)
        ));
    }
    degradations
}

fn format_ray_budget(budget: RayBudgetPolicy) -> String {
    format!(
        "max_distance={:.6} min_step={:.6} hit_epsilon={:.6} max_steps={}",
        budget.max_distance, budget.min_step, budget.hit_epsilon, budget.max_steps
    )
}

fn required_guarantee_class_name(class: RequiredGuaranteeClass) -> &'static str {
    match class {
        RequiredGuaranteeClass::Exact => "exact",
        RequiredGuaranteeClass::ConservativeNoFalseMiss => "conservative_no_false_miss",
        RequiredGuaranteeClass::IntervalBounded => "interval_bounded",
        RequiredGuaranteeClass::BestEffort => "best_effort",
    }
}

fn selected_method_class_name(class: SelectedMethodClass) -> &'static str {
    match class {
        SelectedMethodClass::ExactOracle => "exact_oracle",
        SelectedMethodClass::ConservativeSolver => "conservative_solver",
        SelectedMethodClass::IntervalSolver => "interval_solver",
        SelectedMethodClass::HeuristicSolver => "heuristic_solver",
    }
}

fn support_class_label(class: SupportClass) -> &'static str {
    match class {
        SupportClass::Unknown => "unknown",
        SupportClass::Unbounded => "unbounded",
        SupportClass::Bounded => "bounded",
        SupportClass::Periodic => "periodic",
    }
}

fn stage_label(stage: SemanticStageKind) -> &'static str {
    match stage {
        SemanticStageKind::Execute => "execute",
        SemanticStageKind::CandidateGeneration => "candidate-generation",
        SemanticStageKind::CandidatePruning => "candidate-pruning",
        SemanticStageKind::ArtifactLoad => "artifact-load",
        SemanticStageKind::ParticipantSelection => "participant-selection",
        SemanticStageKind::ItemIteration => "item-iteration",
        SemanticStageKind::DomainSelection => "domain-selection",
        SemanticStageKind::DispatchScaffolding => "dispatch-scaffolding",
        SemanticStageKind::BackendSelection => "backend-selection",
        SemanticStageKind::CaptureLoad => "capture-load",
        SemanticStageKind::HitContextAssembly => "hit-context-assembly",
        SemanticStageKind::ResultAppend => "result-append",
        SemanticStageKind::RaySolver => "ray-solver",
    }
}

fn cost_unit_label(unit: SemanticCostUnit) -> &'static str {
    match unit {
        SemanticCostUnit::CaptureCandidates => "capture-candidates",
        SemanticCostUnit::WorldShapes => "world-shapes",
        SemanticCostUnit::BatchItems => "batch-items",
    }
}

fn cost_fidelity_label(fidelity: CostFidelity) -> &'static str {
    match fidelity {
        CostFidelity::Exact => "exact",
        CostFidelity::StructuralApproximation => "structural-approximation",
    }
}

fn cause_label(kind: SemanticCostCauseKind) -> &'static str {
    match kind {
        SemanticCostCauseKind::MarchPressure => "march-pressure",
        SemanticCostCauseKind::SupportTopology => "support-topology",
        SemanticCostCauseKind::CandidateTraversal => "candidate-traversal",
        SemanticCostCauseKind::CaptureArtifacts => "capture-artifacts",
        SemanticCostCauseKind::DomainGating => "domain-gating",
        SemanticCostCauseKind::ParticipantAccumulation => "participant-accumulation",
        SemanticCostCauseKind::IdentityLocality => "identity-locality",
        SemanticCostCauseKind::OpaqueFallback => "opaque-fallback",
        SemanticCostCauseKind::BackendDispatch => "backend-dispatch",
        SemanticCostCauseKind::RaySolverFallback => "ray-solver-fallback",
    }
}
