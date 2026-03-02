use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidencyPolicy {
    pub scope: String,
    pub allow_localities: BTreeSet<String>,
    pub deny_localities: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelocationJob {
    pub job_id: String,
    pub keyrange: String,
    pub source_home: String,
    pub target_home: String,
    pub reason: String,
    pub phase: RelocationPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelocationPhase {
    Planned,
    Copy,
    DualApply,
    Cutover,
    Finalize,
    RolledBack,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlacementHomeError {
    ResidencyPolicyDenied { locality: String, scope: String },
    PolicyUnsatisfiable,
    InsufficientInBoundaryCapacity,
    KeyrangeMissing(String),
    RelocationMissing(String),
}

#[derive(Debug, Default, Clone)]
pub struct PlacementHomeStore {
    homes: BTreeMap<String, String>,
    relocations: BTreeMap<String, RelocationJob>,
    next_job_id: u64,
}

impl PlacementHomeStore {
    pub fn set_home(
        &mut self,
        keyrange: &str,
        home: &str,
        policy: &ResidencyPolicy,
    ) -> Result<(), PlacementHomeError> {
        enforce_locality(home, policy)?;
        self.homes.insert(keyrange.to_string(), home.to_string());
        Ok(())
    }

    pub fn get_home(&self, keyrange: &str) -> Option<&str> {
        self.homes.get(keyrange).map(String::as_str)
    }

    pub fn relocate_home(
        &mut self,
        keyrange: &str,
        target_home: &str,
        reason: &str,
        policy: &ResidencyPolicy,
    ) -> Result<RelocationJob, PlacementHomeError> {
        enforce_locality(target_home, policy)?;
        if policy.allow_localities.is_empty() {
            return Err(PlacementHomeError::PolicyUnsatisfiable);
        }

        let source_home = self
            .homes
            .get(keyrange)
            .cloned()
            .ok_or_else(|| PlacementHomeError::KeyrangeMissing(keyrange.to_string()))?;

        if source_home == target_home {
            return Err(PlacementHomeError::InsufficientInBoundaryCapacity);
        }

        self.next_job_id = self.next_job_id.saturating_add(1);
        let job_id = format!("reloc-{:06}", self.next_job_id);
        let job = RelocationJob {
            job_id: job_id.clone(),
            keyrange: keyrange.to_string(),
            source_home,
            target_home: target_home.to_string(),
            reason: reason.to_string(),
            phase: RelocationPhase::Planned,
        };
        self.relocations.insert(job_id.clone(), job.clone());
        Ok(job)
    }

    pub fn get_relocation(&self, job_id: &str) -> Result<RelocationJob, PlacementHomeError> {
        self.relocations
            .get(job_id)
            .cloned()
            .ok_or_else(|| PlacementHomeError::RelocationMissing(job_id.to_string()))
    }

    pub fn advance_relocation(
        &mut self,
        job_id: &str,
    ) -> Result<RelocationJob, PlacementHomeError> {
        let job = self
            .relocations
            .get_mut(job_id)
            .ok_or_else(|| PlacementHomeError::RelocationMissing(job_id.to_string()))?;

        job.phase = match job.phase {
            RelocationPhase::Planned => RelocationPhase::Copy,
            RelocationPhase::Copy => RelocationPhase::DualApply,
            RelocationPhase::DualApply => RelocationPhase::Cutover,
            RelocationPhase::Cutover => {
                self.homes
                    .insert(job.keyrange.clone(), job.target_home.clone());
                RelocationPhase::Finalize
            }
            RelocationPhase::Finalize => RelocationPhase::Finalize,
            RelocationPhase::RolledBack => RelocationPhase::RolledBack,
        };

        Ok(job.clone())
    }

    pub fn rollback_relocation(
        &mut self,
        job_id: &str,
    ) -> Result<RelocationJob, PlacementHomeError> {
        let job = self
            .relocations
            .get_mut(job_id)
            .ok_or_else(|| PlacementHomeError::RelocationMissing(job_id.to_string()))?;
        job.phase = RelocationPhase::RolledBack;
        self.homes
            .insert(job.keyrange.clone(), job.source_home.clone());
        Ok(job.clone())
    }
}

fn enforce_locality(home: &str, policy: &ResidencyPolicy) -> Result<(), PlacementHomeError> {
    if policy.deny_localities.contains(home) || !policy.allow_localities.contains(home) {
        return Err(PlacementHomeError::ResidencyPolicyDenied {
            locality: home.to_string(),
            scope: policy.scope.clone(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn us_policy() -> ResidencyPolicy {
        ResidencyPolicy {
            scope: "US".to_string(),
            allow_localities: BTreeSet::from(["us-central".to_string(), "us-east".to_string()]),
            deny_localities: BTreeSet::from(["eu-west".to_string()]),
        }
    }

    #[test]
    fn set_home_fails_closed_for_denied_locality() {
        let mut store = PlacementHomeStore::default();
        let err = store
            .set_home("kr:orders", "eu-west", &us_policy())
            .expect_err("deny");
        assert!(matches!(
            err,
            PlacementHomeError::ResidencyPolicyDenied { .. }
        ));
    }

    #[test]
    fn relocation_advances_deterministically_and_is_resumable() {
        let mut store = PlacementHomeStore::default();
        let policy = us_policy();
        store
            .set_home("kr:orders", "us-central", &policy)
            .expect("set home");
        let job = store
            .relocate_home("kr:orders", "us-east", "rebalance", &policy)
            .expect("relocate");

        let mut current = store.advance_relocation(&job.job_id).expect("copy");
        assert_eq!(current.phase, RelocationPhase::Copy);
        current = store.advance_relocation(&job.job_id).expect("dual");
        assert_eq!(current.phase, RelocationPhase::DualApply);
        current = store.advance_relocation(&job.job_id).expect("cutover");
        assert_eq!(current.phase, RelocationPhase::Cutover);
        current = store.advance_relocation(&job.job_id).expect("finalize");
        assert_eq!(current.phase, RelocationPhase::Finalize);
        assert_eq!(store.get_home("kr:orders"), Some("us-east"));
    }
}
