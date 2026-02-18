use std::collections::BTreeSet;
use wrela_runtime::db::placement::{PlacementHomeStore, RelocationPhase, ResidencyPolicy};

fn policy() -> ResidencyPolicy {
    ResidencyPolicy {
        scope: "US".to_string(),
        allow_localities: BTreeSet::from(["us-central".to_string(), "us-east".to_string()]),
        deny_localities: BTreeSet::from(["eu-west".to_string()]),
    }
}

#[test]
fn keyrange_home_and_relocation_fail_closed_policy_enforcement() {
    let mut store = PlacementHomeStore::default();
    store
        .set_home("kr:tenant-a", "us-central", &policy())
        .expect("set home");
    let err = store.relocate_home("kr:tenant-a", "eu-west", "residency-breach", &policy());
    assert!(err.is_err());
}

#[test]
fn relocation_is_deterministic_and_rollback_safe() {
    let mut store = PlacementHomeStore::default();
    store
        .set_home("kr:tenant-a", "us-central", &policy())
        .expect("set home");
    let job = store
        .relocate_home("kr:tenant-a", "us-east", "capacity", &policy())
        .expect("relocate");

    let _ = store.advance_relocation(&job.job_id).expect("copy");
    let _ = store.advance_relocation(&job.job_id).expect("dual");
    let _ = store.rollback_relocation(&job.job_id).expect("rollback");
    let after = store.get_relocation(&job.job_id).expect("job");
    assert_eq!(after.phase, RelocationPhase::RolledBack);
    assert_eq!(store.get_home("kr:tenant-a"), Some("us-central"));
}
