use std::collections::BTreeSet;
use wrela_runtime::db::shard::migrate::{MigrationPhase, advance_phase, plan_migration, rollback};

#[test]
fn migration_under_write_load_completes_and_records_progress() {
    let allowed = BTreeSet::from(["us".to_string(), "eu".to_string()]);
    let mut plan = plan_migration(
        "mig-001",
        3,
        "us",
        "eu",
        vec!["n-us-1".to_string(), "n-us-2".to_string()],
        vec!["n-eu-1".to_string(), "n-eu-2".to_string()],
        &allowed,
    )
    .expect("plan");

    for writes in [20, 15, 10, 8, 5, 2] {
        advance_phase(&mut plan, writes);
    }

    assert_eq!(plan.phase, MigrationPhase::Completed);
    assert_eq!(plan.writes_observed, 60);
}

#[test]
fn migration_rollback_restores_safe_state() {
    let allowed = BTreeSet::from(["us".to_string(), "eu".to_string()]);
    let mut plan = plan_migration(
        "mig-002",
        4,
        "us",
        "eu",
        vec!["n-us-1".to_string()],
        vec!["n-eu-1".to_string()],
        &allowed,
    )
    .expect("plan");

    advance_phase(&mut plan, 10);
    advance_phase(&mut plan, 10);
    rollback(&mut plan);

    assert_eq!(plan.phase, MigrationPhase::RolledBack);
}
