use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationPlan {
    pub migration_id: String,
    pub shard_id: u32,
    pub source_region: String,
    pub target_region: String,
    pub source_replicas: Vec<String>,
    pub target_replicas: Vec<String>,
    pub phase: MigrationPhase,
    pub writes_observed: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationPhase {
    Planned,
    BootstrapTarget,
    CatchUpTail,
    CutoverOwnership,
    Verify,
    RetireSource,
    Completed,
    RolledBack,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationError {
    ResidencyViolation { source: String, target: String },
    InvalidReplicaSet,
}

pub fn plan_migration(
    migration_id: &str,
    shard_id: u32,
    source_region: &str,
    target_region: &str,
    source_replicas: Vec<String>,
    target_replicas: Vec<String>,
    allowed_regions: &BTreeSet<String>,
) -> Result<MigrationPlan, MigrationError> {
    if source_replicas.is_empty() || target_replicas.is_empty() {
        return Err(MigrationError::InvalidReplicaSet);
    }
    if !allowed_regions.contains(source_region) || !allowed_regions.contains(target_region) {
        return Err(MigrationError::ResidencyViolation {
            source: source_region.to_string(),
            target: target_region.to_string(),
        });
    }

    Ok(MigrationPlan {
        migration_id: migration_id.to_string(),
        shard_id,
        source_region: source_region.to_string(),
        target_region: target_region.to_string(),
        source_replicas,
        target_replicas,
        phase: MigrationPhase::Planned,
        writes_observed: 0,
    })
}

pub fn advance_phase(plan: &mut MigrationPlan, writes_since_last: u64) {
    plan.writes_observed = plan.writes_observed.saturating_add(writes_since_last);
    plan.phase = match plan.phase {
        MigrationPhase::Planned => MigrationPhase::BootstrapTarget,
        MigrationPhase::BootstrapTarget => MigrationPhase::CatchUpTail,
        MigrationPhase::CatchUpTail => MigrationPhase::CutoverOwnership,
        MigrationPhase::CutoverOwnership => MigrationPhase::Verify,
        MigrationPhase::Verify => MigrationPhase::RetireSource,
        MigrationPhase::RetireSource => MigrationPhase::Completed,
        MigrationPhase::Completed => MigrationPhase::Completed,
        MigrationPhase::RolledBack => MigrationPhase::RolledBack,
    };
}

pub fn rollback(plan: &mut MigrationPlan) {
    plan.phase = MigrationPhase::RolledBack;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed() -> BTreeSet<String> {
        BTreeSet::from(["us".to_string(), "eu".to_string()])
    }

    #[test]
    fn rejects_residency_violations_before_movement() {
        let err = plan_migration(
            "m-1",
            7,
            "us",
            "ap",
            vec!["n1".to_string()],
            vec!["n2".to_string()],
            &allowed(),
        )
        .expect_err("must fail");
        assert!(matches!(err, MigrationError::ResidencyViolation { .. }));
    }

    #[test]
    fn phase_progression_is_deterministic_and_resumable() {
        let mut plan = plan_migration(
            "m-2",
            9,
            "us",
            "eu",
            vec!["n1".to_string()],
            vec!["n2".to_string()],
            &allowed(),
        )
        .expect("plan");

        let expected = [
            MigrationPhase::BootstrapTarget,
            MigrationPhase::CatchUpTail,
            MigrationPhase::CutoverOwnership,
            MigrationPhase::Verify,
            MigrationPhase::RetireSource,
            MigrationPhase::Completed,
        ];

        for phase in expected {
            advance_phase(&mut plan, 10);
            assert_eq!(plan.phase, phase);
        }
        assert_eq!(plan.writes_observed, 60);
    }
}
