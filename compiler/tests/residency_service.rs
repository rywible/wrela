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
fn residency_plan_is_deterministic_for_scripted_follow_path() {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("residency"));
    let topology = RegionLine {
        regions: (0..16)
            .map(|idx| candidate(&format!("r{idx}"), idx as f32 * 8.0, 128, 1))
            .collect(),
    };
    let policy = ResidencyPolicy {
        candidate_window: 24.0,
        ..ResidencyPolicy::default()
    };
    let mut left = RegionResidencyService::new(policy.clone(), Box::new(topology.clone()));
    let mut right = RegionResidencyService::new(policy, Box::new(topology));
    for tick in 0..1000 {
        let follow = target((tick % 16) as f32 * 8.0);
        let l = left.plan(follow, &snapshot, SimulationTick::new(tick));
        let r = right.plan(follow, &snapshot, SimulationTick::new(tick));
        assert_eq!(l, r);
        left.apply(&l, SimulationTick::new(tick));
        right.apply(&r, SimulationTick::new(tick));
    }
}

#[test]
fn residency_budget_defers_admits_instead_of_exceeding_budget() {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("residency_budget"));
    let topology = RegionLine {
        regions: vec![
            candidate("a", 0.0, 80, 1),
            candidate("b", 1.0, 80, 1),
            candidate("c", 2.0, 80, 1),
        ],
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
    let plan = service.plan(target(0.0), &snapshot, SimulationTick::new(1));
    assert!(plan.bytes_planned <= 100);
    assert_eq!(plan.admits.len(), 1);
    assert_eq!(plan.deferred.len(), 2);
}

#[test]
fn residency_stale_compatibility_hash_is_evicted_before_re_admit() {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("residency_stale"));
    let topology_v1 = RegionLine {
        regions: vec![candidate("a", 0.0, 80, 1)],
    };
    let mut service = RegionResidencyService::new(
        ResidencyPolicy {
            max_upload_bytes_per_frame: 100,
            max_admits_per_frame: 8,
            max_evicts_per_frame: 8,
            candidate_window: 10.0,
            ..ResidencyPolicy::default()
        },
        Box::new(topology_v1),
    );
    let first = service.plan(target(0.0), &snapshot, SimulationTick::new(1));
    service.apply(&first, SimulationTick::new(1));

    let topology_v2 = RegionLine {
        regions: vec![candidate("a", 0.0, 80, 2)],
    };
    let mut changed = RegionResidencyService::new(
        ResidencyPolicy {
            max_upload_bytes_per_frame: 200,
            max_admits_per_frame: 8,
            max_evicts_per_frame: 8,
            candidate_window: 10.0,
            ..ResidencyPolicy::default()
        },
        Box::new(topology_v2),
    );
    changed.apply(&first, SimulationTick::new(1));
    let plan = changed.plan(target(0.0), &snapshot, SimulationTick::new(2));
    assert!(plan.evicts.iter().any(|evict| evict.stale));
    assert_eq!(plan.admits.len(), 1);
    assert_eq!(plan.bytes_planned, 80);
    assert_eq!(
        plan.bytes_planned,
        plan.admits.iter().map(|admit| admit.bytes).sum::<u64>()
    );
}

#[test]
fn residency_apply_updates_gpu_cache_and_evicts_region_entries() {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("residency_gpu_cache"));
    let topology = RegionLine {
        regions: vec![candidate("a", 0.0, 80, 1)],
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
    let first = service.plan(target(0.0), &snapshot, SimulationTick::new(1));
    let report = service
        .apply_with_gpu_cache(&first, &snapshot, SimulationTick::new(1), &mut gpu_cache)
        .expect("gpu cache apply");
    assert_eq!(report.gpu_cache_misses, 1);
    assert!(gpu_cache.contains_region(&RegionId::new("a")));

    let far = service.plan(target(100.0), &snapshot, SimulationTick::new(2));
    service
        .apply_with_gpu_cache(&far, &snapshot, SimulationTick::new(2), &mut gpu_cache)
        .expect("gpu cache apply");
    assert!(!gpu_cache.contains_region(&RegionId::new("a")));
}
