#[cfg(feature = "internal-learned-experiments")]
use smol_str::SmolStr;
#[cfg(feature = "internal-learned-experiments")]
use tempfile::tempdir;
#[cfg(feature = "internal-learned-experiments")]
use wrela::acceleration::learned::{
    ConservativeNeuralBound, INTERNAL_LEARNED_DATASET_OUT_ENV, LearnedStepProposal,
    build_cpu_oracle_dataset, bypass_learned_step, export_learned_oracle_dataset,
    propose_cpu_oracle_step, verify_learned_step,
};
#[cfg(feature = "internal-learned-experiments")]
use wrela::artifact_store::learned_experiment_store_report;
#[cfg(feature = "internal-learned-experiments")]
use wrela::query_exec::{
    CostFidelity, QueryExecutionObservability, SemanticCostReport, SemanticCostUnit,
    SemanticQueryScope, render_semantic_cost_report,
};
#[cfg(feature = "internal-learned-experiments")]
use wrela::query_plan::{
    CandidateStrategy, DispatchBackend, PlanExecutor, PruningStrategy, QueryItemKind,
};

#[cfg(not(feature = "internal-learned-experiments"))]
#[test]
fn learned_experiments_are_feature_gated_by_default() {
    assert!(!cfg!(feature = "internal-learned-experiments"));
}

#[cfg(feature = "internal-learned-experiments")]
#[test]
fn learned_cpu_oracle_dataset_exports_verifier_rates_and_labels() {
    let subject = SmolStr::new("shape.learned");
    let point = [1.0, 2.0, 3.0];
    let direction = [0.0, 1.0, 0.0];

    let proposal = LearnedStepProposal {
        subject: subject.clone(),
        point,
        direction,
        proposed_step: 0.75,
        no_false_negative_intent: false,
    };
    let bound = ConservativeNeuralBound {
        subject: subject.clone(),
        point,
        conservative_step_bound: 1.0,
        no_false_negative_intent: true,
    };
    let outcome = verify_learned_step(&proposal, &bound, 1.0);
    assert!(outcome.selected);
    assert!(outcome.verified);
    assert!(outcome.accepted);
    assert!(!outcome.rejected);
    assert!(!outcome.bypassed);
    assert!(!outcome.fallback);

    let export = build_cpu_oracle_dataset(
        subject.clone(),
        &proposal,
        &bound,
        &outcome,
        1.0,
        Some(1.0),
        Some([0.0, 1.0]),
    );
    let report = learned_experiment_store_report(&export);
    assert_eq!(report.samples, 1);
    assert_eq!(report.selected, 1);
    assert_eq!(report.verified, 1);
    assert_eq!(report.rejected, 0);
    assert_eq!(report.bypassed, 0);
    assert_eq!(report.verifier_acceptances, 1);
    assert_eq!(report.verifier_fallbacks, 0);
    assert_eq!(report.verifier_acceptance_rate(), Some(1.0));
    assert_eq!(report.verifier_fallback_rate(), Some(0.0));
    assert_eq!(
        export.samples[0].candidate_support_interval,
        Some([0.0, 1.0])
    );
    assert!(export.samples[0].accepted);
    assert!(!export.samples[0].rejected);

    let rejected_proposal = LearnedStepProposal {
        subject: subject.clone(),
        point,
        direction,
        proposed_step: 1.5,
        no_false_negative_intent: false,
    };
    let rejected_bound = ConservativeNeuralBound {
        subject: subject.clone(),
        point,
        conservative_step_bound: 1.0,
        no_false_negative_intent: true,
    };
    let rejected_outcome = verify_learned_step(&rejected_proposal, &rejected_bound, 1.0);
    assert!(rejected_outcome.selected);
    assert!(rejected_outcome.verified);
    assert!(!rejected_outcome.accepted);
    assert!(rejected_outcome.rejected);
    assert!(rejected_outcome.fallback);

    let rejected_export = build_cpu_oracle_dataset(
        subject.clone(),
        &rejected_proposal,
        &rejected_bound,
        &rejected_outcome,
        1.0,
        None,
        None,
    );
    let rejected_report = learned_experiment_store_report(&rejected_export);
    assert_eq!(rejected_report.verifier_acceptance_rate(), Some(0.0));
    assert_eq!(rejected_report.verifier_fallback_rate(), Some(1.0));
    assert!(rejected_report.rejected > 0);

    let bypass = bypass_learned_step();
    assert!(bypass.bypassed);

    let deterministic_left = propose_cpu_oracle_step(subject.clone(), point, direction, 1.0);
    let deterministic_right = propose_cpu_oracle_step(subject, point, direction, 1.0);
    assert_eq!(deterministic_left, deterministic_right);
}

#[cfg(feature = "internal-learned-experiments")]
#[test]
fn learned_cpu_oracle_dataset_export_path_writes_ndjson() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("learned.ndjson");
    let subject = SmolStr::new("shape.learned.export");
    let point = [0.0, 0.0, 0.0];
    let direction = [0.0, 0.0, -1.0];
    let (proposal, bound) = propose_cpu_oracle_step(subject.clone(), point, direction, 1.0);
    let outcome = verify_learned_step(&proposal, &bound, 1.0);
    let export = build_cpu_oracle_dataset(
        subject,
        &proposal,
        &bound,
        &outcome,
        1.0,
        Some(1.0),
        Some([0.0, 1.0]),
    );

    export_learned_oracle_dataset(&path, &export).expect("export dataset");

    let written = std::fs::read_to_string(&path).expect("read learned dataset");
    assert!(written.contains("\"samples\""));
    assert!(written.contains("\"dense_oracle_hit_distance\":1.0"));
    assert_ne!(INTERNAL_LEARNED_DATASET_OUT_ENV, "");
}

#[cfg(feature = "internal-learned-experiments")]
#[test]
fn learned_collision_policy_requires_conservative_intent() {
    use wrela::acceleration::learned::LearnedMethodPolicy;
    use wrela::artifact_contract::ArtifactObserver;
    use wrela::artifact_contract::validate_learned_method_policy;

    assert_eq!(
        validate_learned_method_policy(
            ArtifactObserver::Collision,
            LearnedMethodPolicy::ProposalOnly,
        ),
        vec!["collision learned methods must be conservative and verifier-backed".to_string()]
    );
    assert!(
        validate_learned_method_policy(
            ArtifactObserver::Collision,
            LearnedMethodPolicy::ConservativeNoFalseNegative,
        )
        .is_empty()
    );
}

#[cfg(feature = "internal-learned-experiments")]
#[test]
fn learned_semantic_cost_report_surfaces_verifier_counters_and_rates() {
    let mut counters = QueryExecutionObservability::default();
    counters.learned_step_selected = 4;
    counters.learned_step_verified = 3;
    counters.learned_step_rejected = 1;
    counters.learned_step_bypassed = 2;
    counters.learned_verifier_acceptances = 2;
    counters.learned_verifier_fallbacks = 1;

    let report = SemanticCostReport {
        scope: SemanticQueryScope::World {
            kind: wrela::query_plan::WorldQueryKind::Trace,
        },
        backend: DispatchBackend::Cpu,
        executor: PlanExecutor::WorldTraceCapture,
        item_kind: QueryItemKind::RayQuery,
        unit: SemanticCostUnit::WorldShapes,
        fidelity: CostFidelity::Exact,
        candidate_strategy: CandidateStrategy::ShapeBranchTraversal,
        pruning_strategy: PruningStrategy::SupportLowerBound,
        preserves_local_hit_context: true,
        scene: None,
        artifact_labels: Vec::new(),
        domain_flags: Vec::new(),
        execution_policy: None,
        execution_degradations: Vec::new(),
        dominant_stages: Vec::new(),
        causes: Vec::new(),
        counters,
    };

    let rendered = render_semantic_cost_report(&report);
    assert!(rendered.contains("learned_step_selected=4"));
    assert!(rendered.contains("learned_step_verified=3"));
    assert!(rendered.contains("learned_step_rejected=1"));
    assert!(rendered.contains("learned_step_bypassed=2"));
    assert!(rendered.contains("learned_verifier_acceptances=2"));
    assert!(rendered.contains("learned_verifier_fallbacks=1"));
    assert!(rendered.contains("learned_verifier_acceptance_rate=0.667"));
    assert!(rendered.contains("learned_verifier_fallback_rate=0.250"));
}
