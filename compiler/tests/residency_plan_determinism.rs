use smol_str::SmolStr;
use wrela::query_exec::stable_region_snapshot_handle;
use wrela::residency::follow::{FollowTarget, Transform3, Velocity3};
use wrela::residency::{
    RegionId, RegionLine, RegionResidencyService, ResidencyCandidate, ResidencyPolicy,
};
use wrela::state_advance::SimulationTick;

fn candidate(id: &str, x: f32, bytes: u64, hash: u64) -> ResidencyCandidate {
    ResidencyCandidate {
        region_id: RegionId::new(id),
        center: [x, 0.0, 0.0],
        bytes,
        compatibility_hash: hash,
    }
}

#[test]
fn residency_prediction_uses_velocity_times_policy_horizon() {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("residency_prediction_horizon"));
    let topology = RegionLine {
        regions: vec![
            candidate("near_predicted_frame", 1.6, 64, 1),
            candidate("one_second_ahead", 100.0, 64, 1),
        ],
    };
    let mut service = RegionResidencyService::new(
        ResidencyPolicy {
            prediction_horizon_secs: 0.016,
            candidate_window: 5.0,
            max_admits_per_frame: 8,
            ..ResidencyPolicy::default()
        },
        Box::new(topology),
    );
    let plan = service.plan(
        FollowTarget {
            transform: Transform3 {
                translation: [0.0, 0.0, 0.0],
            },
            velocity: Some(Velocity3 {
                meters_per_second: [100.0, 0.0, 0.0],
            }),
        },
        &snapshot,
        SimulationTick::new(1),
    );

    assert_eq!(
        plan.admits
            .iter()
            .map(|admit| admit.region_id.clone())
            .collect::<Vec<_>>(),
        vec![RegionId::new("near_predicted_frame")]
    );
}
