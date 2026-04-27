use smol_str::SmolStr;
use wrela::query_exec::stable_region_snapshot_handle;
use wrela::residency::follow::{FollowTarget, Transform3};
use wrela::residency::{
    RegionId, RegionLine, RegionResidencyService, ResidencyCandidate, ResidencyGpuCache,
    ResidencyPolicy,
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
fn residency_re_admit_consumes_evict_admit_and_upload_budget() {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("residency_readmit_budget"));
    let first_topology = RegionLine {
        regions: vec![candidate("changed", 0.0, 80, 1)],
    };
    let mut service = RegionResidencyService::new(
        ResidencyPolicy {
            max_upload_bytes_per_frame: 80,
            max_admits_per_frame: 1,
            max_evicts_per_frame: 1,
            candidate_window: 10.0,
            ..ResidencyPolicy::default()
        },
        Box::new(first_topology),
    );
    let first = service.plan(target(0.0), &snapshot, SimulationTick::new(1));
    service.apply(&first, SimulationTick::new(1));

    let changed_topology = RegionLine {
        regions: vec![
            candidate("changed", 0.0, 80, 2),
            candidate("also_desired", 1.0, 1, 1),
        ],
    };
    let mut changed =
        RegionResidencyService::new(service.policy.clone(), Box::new(changed_topology));
    changed.apply(&first, SimulationTick::new(1));
    let plan = changed.plan(target(0.0), &snapshot, SimulationTick::new(2));

    assert_eq!(plan.evicts.len(), 1);
    assert_eq!(plan.admits.len(), 1);
    assert_eq!(plan.bytes_planned, 80);
    assert_eq!(plan.deferred, vec![RegionId::new("also_desired")]);
}

#[test]
fn residency_gpu_cache_insert_errors_are_returned() {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("residency_gpu_error"));
    let topology = RegionLine {
        regions: vec![candidate("region", 0.0, 80, 1)],
    };
    let mut service = RegionResidencyService::new(
        ResidencyPolicy {
            max_upload_bytes_per_frame: 100,
            max_admits_per_frame: 8,
            max_evicts_per_frame: 8,
            candidate_window: 10.0,
            ..ResidencyPolicy::default()
        },
        Box::new(topology),
    );
    let mut gpu_cache = ResidencyGpuCache::default();
    gpu_cache.fail_next_insert_for_test("synthetic upload failure");
    let plan = service.plan(target(0.0), &snapshot, SimulationTick::new(1));
    let err = service
        .apply_with_gpu_cache(&plan, &snapshot, SimulationTick::new(1), &mut gpu_cache)
        .expect_err("GPU cache insert should propagate");

    assert_eq!(
        err.to_string(),
        "residency GPU cache insert failed: synthetic upload failure"
    );
}
