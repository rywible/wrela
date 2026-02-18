use wrela_runtime::db::autopilot::compiler::{
    BudgetClass, CostTier, PolicyCompilerError, PolicyContradictionCode, PolicyIntentSpec,
    SurvivabilityIntent, compile_policy_intent,
};

#[test]
fn deterministic_compile_normalizes_and_hashes_stably() {
    let spec = PolicyIntentSpec {
        policy_id: " orders-v2 ".to_string(),
        survivability: SurvivabilityIntent::RegionFailure,
        latency_target_ms: 80,
        residency_scope: vec![
            " eu-west ".to_string(),
            "us-east".to_string(),
            "ap-south".to_string(),
        ],
        cost_tier: CostTier::Balanced,
        budget_class: BudgetClass::Standard,
    };

    let compiled_a = compile_policy_intent(&spec).expect("compile");
    let compiled_b = compile_policy_intent(&spec).expect("compile");

    assert_eq!(compiled_a, compiled_b);
    assert_eq!(
        compiled_a.residency_scope,
        vec![
            "ap-south".to_string(),
            "eu-west".to_string(),
            "us-east".to_string()
        ]
    );
}

#[test]
fn contradictory_policy_is_rejected_with_typed_reason() {
    let spec = PolicyIntentSpec {
        policy_id: "orders-v2".to_string(),
        survivability: SurvivabilityIntent::TwoRegionFailure,
        latency_target_ms: 85,
        residency_scope: vec!["us-east".to_string(), "eu-west".to_string()],
        cost_tier: CostTier::Balanced,
        budget_class: BudgetClass::Standard,
    };

    let err = compile_policy_intent(&spec).expect_err("contradiction");
    match err {
        PolicyCompilerError::Contradiction(conflict) => {
            assert_eq!(
                conflict.code,
                PolicyContradictionCode::SurvivabilityExceedsResidency
            );
            assert!(conflict.reason.contains("requires at least 3"));
        }
        other => panic!("expected contradiction, got {:?}", other),
    }
}

#[test]
fn explain_metadata_is_stable_for_equivalent_inputs() {
    let raw_a = PolicyIntentSpec {
        policy_id: "orders-v2".to_string(),
        survivability: SurvivabilityIntent::RegionFailure,
        latency_target_ms: 80,
        residency_scope: vec!["us-east".to_string(), "eu-west".to_string()],
        cost_tier: CostTier::Balanced,
        budget_class: BudgetClass::Standard,
    };
    let raw_b = PolicyIntentSpec {
        policy_id: " orders-v2 ".to_string(),
        survivability: SurvivabilityIntent::RegionFailure,
        latency_target_ms: 80,
        residency_scope: vec![" eu-west ".to_string(), "us-east".to_string()],
        cost_tier: CostTier::Balanced,
        budget_class: BudgetClass::Standard,
    };

    let compiled_a = compile_policy_intent(&raw_a).expect("compile");
    let compiled_b = compile_policy_intent(&raw_b).expect("compile");

    assert_eq!(compiled_a.policy_hash, compiled_b.policy_hash);
    assert_eq!(compiled_a.explain, compiled_b.explain);
    assert_eq!(
        compiled_a.explain.canonical_material,
        "v=1|policy_id=orders-v2|survivability=region_failure|latency_target_ms=80|residency_scope=eu-west,us-east|cost_tier=balanced|budget_class=standard"
    );
}
