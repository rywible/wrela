use wrela_runtime::db::placement::{
    FailureDomain, PlacementProfile, plan_placement, survives_region_loss,
};

fn domains() -> Vec<FailureDomain> {
    vec![
        FailureDomain {
            region: "us-central".to_string(),
            zone: "a".to_string(),
        },
        FailureDomain {
            region: "eu-west".to_string(),
            zone: "a".to_string(),
        },
        FailureDomain {
            region: "ap-south".to_string(),
            zone: "a".to_string(),
        },
    ]
}

#[test]
fn default_three_region_profile_survives_single_region_loss() {
    let plan = plan_placement(PlacementProfile::ThreeRegionSurvivability, &domains(), None)
        .expect("placement plan");
    assert_eq!(plan.replicas.len(), 3);
    assert!(survives_region_loss(&plan, "us-central"));
    assert!(survives_region_loss(&plan, "eu-west"));
}

#[test]
fn single_region_profile_is_explicit_and_deterministic() {
    let plan = plan_placement(
        PlacementProfile::SingleRegion,
        &domains(),
        Some("us-central"),
    )
    .expect("placement plan");
    assert_eq!(plan.replicas.len(), 3);
    assert!(
        plan.replicas
            .iter()
            .all(|replica| replica.region == "us-central")
    );
}
