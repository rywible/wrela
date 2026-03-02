use crate::db::backup::{
    BackupManifestEnvelope, RestorePlanError, SnapshotMetadata,
    verify_snapshot_manifest_consistency,
};
use crate::db::snapshot::manifest::{SnapshotManifest, SnapshotValidationError};

#[derive(Debug, Clone)]
pub struct RestoreLoadRequest {
    pub source_uri: String,
    pub expected_snapshot: SnapshotMetadata,
    pub snapshot_manifest: SnapshotManifest,
    pub snapshot_payload: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ValidatedRestoreLoad {
    pub source_uri: String,
    pub snapshot_manifest: SnapshotManifest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreValidationError {
    Plan(RestorePlanError),
    Manifest(SnapshotValidationError),
}

pub fn validate_restore_load_request(
    request: &RestoreLoadRequest,
) -> Result<ValidatedRestoreLoad, RestoreValidationError> {
    let envelope =
        BackupManifestEnvelope::new(request.source_uri.clone(), request.expected_snapshot);
    verify_snapshot_manifest_consistency(&envelope, &request.snapshot_manifest)
        .map_err(RestoreValidationError::Plan)?;

    request
        .snapshot_manifest
        .validate_payload(&request.snapshot_payload)
        .map_err(RestoreValidationError::Manifest)?;

    Ok(ValidatedRestoreLoad {
        source_uri: request.source_uri.clone(),
        snapshot_manifest: request.snapshot_manifest.clone(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatchUpPhase {
    InstallSnapshot {
        snapshot_index: u64,
        snapshot_term: u64,
    },
    ReplayTail {
        start_index: u64,
        end_index: u64,
    },
    Steady,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatchUpPlan {
    pub phases: Vec<CatchUpPhase>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatchUpPlannerError {
    FollowerAhead {
        follower_last_index: u64,
        leader_last_index: u64,
    },
}

pub fn plan_catch_up_phases(
    follower_last_index: u64,
    leader_snapshot_index: u64,
    leader_snapshot_term: u64,
    leader_last_index: u64,
) -> Result<CatchUpPlan, CatchUpPlannerError> {
    if follower_last_index > leader_last_index {
        return Err(CatchUpPlannerError::FollowerAhead {
            follower_last_index,
            leader_last_index,
        });
    }

    let mut phases = Vec::new();

    if follower_last_index < leader_snapshot_index {
        phases.push(CatchUpPhase::InstallSnapshot {
            snapshot_index: leader_snapshot_index,
            snapshot_term: leader_snapshot_term,
        });

        if leader_snapshot_index < leader_last_index {
            phases.push(CatchUpPhase::ReplayTail {
                start_index: leader_snapshot_index.saturating_add(1),
                end_index: leader_last_index,
            });
        }
    } else if follower_last_index < leader_last_index {
        phases.push(CatchUpPhase::ReplayTail {
            start_index: follower_last_index.saturating_add(1),
            end_index: leader_last_index,
        });
    }

    phases.push(CatchUpPhase::Steady);
    Ok(CatchUpPlan { phases })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreStatus {
    Requested,
    Validating,
    SnapshotInstalled,
    TailReplayed,
    Steady,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestoreStatusTransition {
    pub from: RestoreStatus,
    pub to: RestoreStatus,
}

#[derive(Debug, Clone)]
pub struct RestoreOutcome {
    pub validated: ValidatedRestoreLoad,
    pub catch_up_plan: CatchUpPlan,
    pub transitions: Vec<RestoreStatusTransition>,
    pub final_status: RestoreStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreOrchestrationError {
    Validation(RestoreValidationError),
    CatchUp(CatchUpPlannerError),
}

pub fn orchestrate_restore(
    request: &RestoreLoadRequest,
    follower_last_index: u64,
    leader_last_index: u64,
) -> Result<RestoreOutcome, RestoreOrchestrationError> {
    let mut current = RestoreStatus::Requested;
    let mut transitions = Vec::new();

    transitions.push(RestoreStatusTransition {
        from: current,
        to: RestoreStatus::Validating,
    });
    current = RestoreStatus::Validating;

    let validated = match validate_restore_load_request(request) {
        Ok(validated) => validated,
        Err(err) => {
            transitions.push(RestoreStatusTransition {
                from: current,
                to: RestoreStatus::Failed,
            });
            return Err(RestoreOrchestrationError::Validation(err));
        }
    };

    let catch_up_plan = match plan_catch_up_phases(
        follower_last_index,
        request.snapshot_manifest.last_index,
        request.snapshot_manifest.last_term,
        leader_last_index,
    ) {
        Ok(plan) => plan,
        Err(err) => {
            transitions.push(RestoreStatusTransition {
                from: current,
                to: RestoreStatus::Failed,
            });
            return Err(RestoreOrchestrationError::CatchUp(err));
        }
    };

    for phase in &catch_up_plan.phases {
        let next = match phase {
            CatchUpPhase::InstallSnapshot { .. } => RestoreStatus::SnapshotInstalled,
            CatchUpPhase::ReplayTail { .. } => RestoreStatus::TailReplayed,
            CatchUpPhase::Steady => RestoreStatus::Steady,
        };

        if next != current {
            transitions.push(RestoreStatusTransition {
                from: current,
                to: next,
            });
            current = next;
        }
    }

    Ok(RestoreOutcome {
        validated,
        catch_up_plan,
        transitions,
        final_status: current,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::snapshot::builder::build_manifest;

    fn request(
        source_uri: &str,
        payload: &[u8],
        manifest: SnapshotManifest,
        expected: SnapshotMetadata,
    ) -> RestoreLoadRequest {
        RestoreLoadRequest {
            source_uri: source_uri.to_string(),
            expected_snapshot: expected,
            snapshot_manifest: manifest,
            snapshot_payload: payload.to_vec(),
        }
    }

    #[test]
    fn success_path_orchestrates_validation_to_steady() {
        let payload = b"restore-seed-payload";
        let manifest = build_manifest(payload, 42, 7);
        let req = request(
            "s3://bucket/snapshots/42",
            payload,
            manifest.clone(),
            SnapshotMetadata {
                last_index: manifest.last_index,
                last_term: manifest.last_term,
                checksum: manifest.checksum,
            },
        );

        let outcome = orchestrate_restore(&req, 10, 55).expect("orchestration should succeed");

        assert_eq!(outcome.final_status, RestoreStatus::Steady);
        assert_eq!(
            outcome.catch_up_plan.phases,
            vec![
                CatchUpPhase::InstallSnapshot {
                    snapshot_index: 42,
                    snapshot_term: 7,
                },
                CatchUpPhase::ReplayTail {
                    start_index: 43,
                    end_index: 55,
                },
                CatchUpPhase::Steady,
            ]
        );
        assert_eq!(
            outcome.transitions,
            vec![
                RestoreStatusTransition {
                    from: RestoreStatus::Requested,
                    to: RestoreStatus::Validating,
                },
                RestoreStatusTransition {
                    from: RestoreStatus::Validating,
                    to: RestoreStatus::SnapshotInstalled,
                },
                RestoreStatusTransition {
                    from: RestoreStatus::SnapshotInstalled,
                    to: RestoreStatus::TailReplayed,
                },
                RestoreStatusTransition {
                    from: RestoreStatus::TailReplayed,
                    to: RestoreStatus::Steady,
                },
            ]
        );
    }

    #[test]
    fn invalid_manifest_fails_pre_activation_validation() {
        let payload = b"restore-seed-payload";
        let manifest = build_manifest(payload, 42, 7);
        let actual_checksum = manifest.checksum;
        let req = request(
            "s3://bucket/snapshots/42",
            payload,
            manifest,
            SnapshotMetadata {
                last_index: 42,
                last_term: 7,
                checksum: 99,
            },
        );

        let err = orchestrate_restore(&req, 10, 55)
            .expect_err("checksum mismatch should fail before activation");

        assert_eq!(
            err,
            RestoreOrchestrationError::Validation(RestoreValidationError::Plan(
                RestorePlanError::SnapshotChecksumMismatch {
                    expected: 99,
                    actual: actual_checksum,
                }
            ))
        );
    }

    #[test]
    fn catch_up_phase_transitions_are_deterministic() {
        assert_eq!(
            plan_catch_up_phases(5, 20, 3, 25)
                .expect("plan should build")
                .phases,
            vec![
                CatchUpPhase::InstallSnapshot {
                    snapshot_index: 20,
                    snapshot_term: 3,
                },
                CatchUpPhase::ReplayTail {
                    start_index: 21,
                    end_index: 25,
                },
                CatchUpPhase::Steady,
            ]
        );

        assert_eq!(
            plan_catch_up_phases(20, 20, 3, 25)
                .expect("plan should build")
                .phases,
            vec![
                CatchUpPhase::ReplayTail {
                    start_index: 21,
                    end_index: 25,
                },
                CatchUpPhase::Steady,
            ]
        );

        assert_eq!(
            plan_catch_up_phases(25, 20, 3, 25)
                .expect("plan should build")
                .phases,
            vec![CatchUpPhase::Steady]
        );
    }
}
