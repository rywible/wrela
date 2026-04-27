use smol_str::SmolStr;
use wrela::query_exec::stable_region_snapshot_handle;
use wrela::residency::follow::{FollowTarget, Transform3};
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

fn target(x: f32) -> FollowTarget {
    FollowTarget {
        transform: Transform3 {
            translation: [x, 0.0, 0.0],
        },
        velocity: None,
    }
}

#[test]
fn incompatible_residents_are_stale_but_outside_window_evictions_are_lru() {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("residency_stale_priority"));
    let first_topology = RegionLine {
        regions: vec![
            candidate("stale_hash", 0.0, 80, 1),
            candidate("outside_window", 1.0, 80, 1),
        ],
    };
    let policy = ResidencyPolicy {
        max_upload_bytes_per_frame: 200,
        max_admits_per_frame: 4,
        max_evicts_per_frame: 4,
        candidate_window: 10.0,
        ..ResidencyPolicy::default()
    };
    let mut service = RegionResidencyService::new(policy.clone(), Box::new(first_topology));
    let first = service.plan(target(0.0), &snapshot, SimulationTick::new(1));
    service.apply(&first, SimulationTick::new(1));

    let changed_topology = RegionLine {
        regions: vec![candidate("stale_hash", 0.0, 80, 2)],
    };
    let mut changed = RegionResidencyService::new(policy, Box::new(changed_topology));
    changed.apply(&first, SimulationTick::new(1));
    let plan = changed.plan(target(0.0), &snapshot, SimulationTick::new(2));

    assert_eq!(
        plan.evicts,
        vec![
            wrela::residency::ResidencyEvict {
                region_id: RegionId::new("stale_hash"),
                stale: true,
            },
            wrela::residency::ResidencyEvict {
                region_id: RegionId::new("outside_window"),
                stale: false,
            },
        ]
    );
}
