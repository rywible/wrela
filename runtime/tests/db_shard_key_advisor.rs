use wrela_runtime::db::schema_evolution::{CutoverReadiness, ValidationMismatch};
use wrela_runtime::db::shard::advisor::{
    ShardKeyTelemetryProfile, ShardKeyTelemetrySample, compiler_feedback, explain, profile,
    profile_coverage, recommend,
};
use wrela_runtime::db::shard::evolution::{
    EvolutionPlanInput, ExecutionPhase, ExecutionState, evaluate_readiness, plan, simulate,
};
use wrela_runtime::db::shard::gates::{
    GateThresholds, evaluate_gates, evaluate_plan_inputs, perf_budget_summary,
};

#[test]
fn telemetry_profiler_and_explain_surface_are_stable() {
    let samples = vec![
        ShardKeyTelemetrySample {
            relation: "orders".to_string(),
            key_spec: "region".to_string(),
            shard_id: 1,
            read_count: 900,
            write_count: 300,
            distinct_keys_observed: 20,
        },
        ShardKeyTelemetrySample {
            relation: "orders".to_string(),
            key_spec: "region".to_string(),
            shard_id: 2,
            read_count: 70,
            write_count: 30,
            distinct_keys_observed: 8,
        },
    ];

    let prof = profile(&samples).expect("profile");
    let rec = recommend(&prof);
    let exp = explain(rec.clone(), &prof);

    assert_eq!(exp.recommendation, rec);
    assert!(exp.evidence.contains_key("skew_per_mille"));
    assert!(prof.skew_per_mille > 800);

    let cov = profile_coverage(&samples);
    assert_eq!(cov.get("orders"), Some(&2usize));
}

#[test]
fn recommendation_and_compiler_feedback_emit_actionable_signals() {
    let prof = ShardKeyTelemetryProfile {
        relation: "events".to_string(),
        key_spec: "region".to_string(),
        shard_count: 3,
        total_reads: 12_000,
        total_writes: 4_000,
        total_observations: 16_000,
        hottest_shard: 2,
        hottest_shard_ops: 11_000,
        coldest_shard: 1,
        coldest_shard_ops: 2_000,
        skew_per_mille: 687,
        cardinality_ratio_per_mille: 30,
    };

    let rec = recommend(&prof);
    assert!(rec.risk_score_per_mille > 0);
    assert!(!rec.reasons.is_empty());

    let feedback = compiler_feedback("events", "region", None);
    assert!(!feedback.is_empty());
}

#[test]
fn planner_simulator_executor_and_readiness_workflow() {
    let p = plan(&EvolutionPlanInput {
        relation: "orders".to_string(),
        from_key: "region".to_string(),
        to_key: "region,user_id".to_string(),
        current_qps: 12_000,
        current_write_qps: 3_200,
        estimated_backfill_rows: 2_400_000,
        estimated_distinct_new_key_values: 40_000,
    })
    .expect("plan");
    let sim = simulate(&p);
    assert!(sim["predicted_copy_seconds"] > 0);

    let mut st = ExecutionState::new("orders", 1000);
    st.advance_copy(500).expect("copy1");
    st.advance_copy(500).expect("copy2");
    assert_eq!(st.phase, ExecutionPhase::DualWrite);
    st.ack_dual_write(300).expect("dual");
    st.cutover(33).expect("cutover");
    st.finalize().expect("finalize");
    assert_eq!(st.phase, ExecutionPhase::Complete);

    let readiness = evaluate_readiness(1000, 1000, 0, vec![], vec![]);
    assert_eq!(readiness, CutoverReadiness::Ready);

    let not_ready = evaluate_readiness(
        1000,
        500,
        0,
        vec![ValidationMismatch::MissingRow {
            row_key: "k1".to_string(),
        }],
        vec![],
    );
    assert!(matches!(not_ready, CutoverReadiness::NotReady { .. }));
}

#[test]
fn conformance_and_perf_gates_fail_and_pass_as_expected() {
    let profiles = vec![ShardKeyTelemetryProfile {
        relation: "orders".to_string(),
        key_spec: "region".to_string(),
        shard_count: 2,
        total_reads: 1000,
        total_writes: 100,
        total_observations: 1100,
        hottest_shard: 1,
        hottest_shard_ops: 1000,
        coldest_shard: 2,
        coldest_shard_ops: 100,
        skew_per_mille: 909,
        cardinality_ratio_per_mille: 20,
    }];

    let plan_a = plan(&EvolutionPlanInput {
        relation: "orders".to_string(),
        from_key: "region".to_string(),
        to_key: "region,user_id".to_string(),
        current_qps: 10,
        current_write_qps: 9,
        estimated_backfill_rows: 10_000_000,
        estimated_distinct_new_key_values: 100,
    })
    .expect("plan");

    let report_fail = evaluate_gates(
        &profiles,
        &[plan_a.clone()],
        &GateThresholds {
            max_advisor_risk_per_mille: 500,
            max_copy_window_seconds: 3600,
            max_dual_write_overhead_per_mille: 400,
        },
    );
    assert!(!report_fail.ok);

    let report_pass = evaluate_plan_inputs(
        &profiles,
        &[EvolutionPlanInput {
            relation: "orders".to_string(),
            from_key: "region".to_string(),
            to_key: "region,user_id".to_string(),
            current_qps: 20_000,
            current_write_qps: 200,
            estimated_backfill_rows: 20_000,
            estimated_distinct_new_key_values: 10_000,
        }],
        &GateThresholds {
            max_advisor_risk_per_mille: 1000,
            max_copy_window_seconds: 20_000,
            max_dual_write_overhead_per_mille: 1000,
        },
    );
    assert!(report_pass.ok);

    let (total_copy, max_overhead) = perf_budget_summary(&[plan_a]);
    assert!(total_copy > 0);
    assert!(max_overhead > 0);
}
