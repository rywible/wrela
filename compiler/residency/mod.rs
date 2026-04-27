//! Region residency follow-target service (RFC 0011 Phase 66).

#![forbid(unsafe_code)]

use crate::artifact_key::{ArtifactPolicyDigestMode, ArtifactReuseKey};
use crate::gpu_runtime::{
    GpuLayoutIdentity, GpuResidentScene, GpuResidentSceneCache, GpuResidentSceneKey,
};
use crate::state_advance::SimulationTick;
use crate::world_identity::WorldSnapshotHandle;
use smol_str::SmolStr;
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use thiserror::Error;

pub mod follow {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct Transform3 {
        pub translation: [f32; 3],
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct Velocity3 {
        pub meters_per_second: [f32; 3],
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct FollowTarget {
        pub transform: Transform3,
        pub velocity: Option<Velocity3>,
    }
}

use follow::FollowTarget;

#[derive(Debug, Clone, PartialEq)]
pub struct ResidencyPolicy {
    pub max_upload_bytes_per_frame: u64,
    pub max_admits_per_frame: u32,
    pub max_evicts_per_frame: u32,
    pub candidate_window: f32,
    pub prediction_horizon_secs: f32,
}

impl Default for ResidencyPolicy {
    fn default() -> Self {
        Self {
            max_upload_bytes_per_frame: 2_000_000,
            max_admits_per_frame: 4,
            max_evicts_per_frame: 4,
            candidate_window: 120.0,
            prediction_horizon_secs: 1.0 / 60.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RegionId(pub SmolStr);

impl RegionId {
    pub fn new(value: impl Into<SmolStr>) -> Self {
        Self(value.into())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResidencyCandidate {
    pub region_id: RegionId,
    pub center: [f32; 3],
    pub bytes: u64,
    pub compatibility_hash: u64,
}

impl ResidencyCandidate {
    pub fn reuse_key(&self, snapshot: &WorldSnapshotHandle) -> ArtifactReuseKey {
        ArtifactReuseKey::new(
            snapshot,
            Some(self.region_id.0.clone()),
            SmolStr::new("resident-region"),
            self.compatibility_hash,
            None,
            ArtifactPolicyDigestMode::None,
        )
    }
}

pub trait ResidencyTopology: Send + Sync {
    fn candidates_for(
        &self,
        target: &FollowTarget,
        window: f32,
        prediction_horizon_secs: f32,
    ) -> Vec<ResidencyCandidate>;
}

#[derive(Debug, Clone)]
pub struct RegionLine {
    pub regions: Vec<ResidencyCandidate>,
}

impl ResidencyTopology for RegionLine {
    fn candidates_for(
        &self,
        target: &FollowTarget,
        window: f32,
        prediction_horizon_secs: f32,
    ) -> Vec<ResidencyCandidate> {
        let predicted = predicted_position(target, prediction_horizon_secs);
        self.regions
            .iter()
            .filter(|region| distance(region.center, predicted) <= window)
            .cloned()
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResidentRegionState {
    pub region_id: RegionId,
    pub reuse_key: ArtifactReuseKey,
    pub resident_since: SimulationTick,
    pub last_touched: SimulationTick,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResidencyAdmit {
    pub region_id: RegionId,
    pub reuse_key: ArtifactReuseKey,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidencyEvict {
    pub region_id: RegionId,
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ResidencyPlan {
    pub admits: Vec<ResidencyAdmit>,
    pub evicts: Vec<ResidencyEvict>,
    pub unchanged: Vec<RegionId>,
    pub deferred: Vec<RegionId>,
    pub bytes_planned: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResidencyReport {
    pub admitted: u32,
    pub evicted: u32,
    pub unchanged: u32,
    pub deferred: u32,
    pub bytes_uploaded: u64,
    pub gpu_cache_hits: u32,
    pub gpu_cache_misses: u32,
    pub admitted_region_ids: Vec<RegionId>,
    pub evicted_region_ids: Vec<RegionId>,
    pub resident_region_ids: Vec<RegionId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ResidencyError {
    #[error("residency GPU cache insert failed: {0}")]
    GpuCacheInsert(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidentRegionGpuPayload {
    pub region_id: RegionId,
    pub bytes: u64,
    pub compatibility_hash: u64,
}

#[derive(Debug, Default)]
pub struct ResidencyGpuCache {
    cache: GpuResidentSceneCache<ResidentRegionGpuPayload>,
    keys_by_region: BTreeMap<RegionId, GpuResidentSceneKey>,
    fail_next_insert: Option<String>,
}

impl ResidencyGpuCache {
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    pub fn contains_region(&self, region_id: &RegionId) -> bool {
        self.keys_by_region
            .get(region_id)
            .and_then(|key| self.cache.get(key))
            .is_some()
    }

    pub fn fail_next_insert_for_test(&mut self, reason: impl Into<String>) {
        self.fail_next_insert = Some(reason.into());
    }

    fn key_for(snapshot: &WorldSnapshotHandle, admit: &ResidencyAdmit) -> GpuResidentSceneKey {
        let layout = GpuLayoutIdentity::new(admit.reuse_key.compatibility_hash, 0);
        GpuResidentSceneKey::new(snapshot.report(), 0, layout)
            .with_selection_signature(region_selection_signature(&admit.region_id))
    }
}

pub struct RegionResidencyService {
    pub policy: ResidencyPolicy,
    topology: Box<dyn ResidencyTopology>,
    resident: BTreeMap<RegionId, ResidentRegionState>,
}

impl RegionResidencyService {
    pub fn new(policy: ResidencyPolicy, topology: Box<dyn ResidencyTopology>) -> Self {
        Self {
            policy,
            topology,
            resident: BTreeMap::new(),
        }
    }

    pub fn plan(
        &mut self,
        target: FollowTarget,
        snapshot: &WorldSnapshotHandle,
        tick: SimulationTick,
    ) -> ResidencyPlan {
        // RFC 0011 H2 acceptance: residency planning is deterministic and
        // budget-respecting.
        //
        // 1. Sort candidates by predicted-distance to the follow target, so
        //    when budgets are tight we keep/admit the most-useful regions.
        //    Ties break on region id for determinism.
        // 2. `bytes_planned` only counts *upload* bytes from admits.
        //    Evictions do not consume the upload budget.
        // 3. Re-admit (incompatible reuse-key) consumes one admit and one
        //    eviction slot, but only `candidate.bytes` of upload budget.
        // 4. Stale evictions (resident regions absent from the desired set)
        //    fill the remaining eviction budget, ordered by oldest
        //    `last_touched` first.
        let mut candidates = self.topology.candidates_for(
            &target,
            self.policy.candidate_window,
            self.policy.prediction_horizon_secs,
        );
        let predicted = predicted_position(&target, self.policy.prediction_horizon_secs);
        candidates.sort_by(|a, b| {
            let da = distance(a.center, predicted);
            let db = distance(b.center, predicted);
            da.partial_cmp(&db)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.region_id.cmp(&b.region_id))
        });
        let desired_ids = candidates
            .iter()
            .map(|candidate| candidate.region_id.clone())
            .collect::<BTreeSet<_>>();
        let mut plan = ResidencyPlan::default();
        for candidate in candidates {
            let reuse_key = candidate.reuse_key(snapshot);
            let admit_budget_remaining =
                plan.admits.len() < self.policy.max_admits_per_frame as usize;
            let evict_budget_remaining =
                plan.evicts.len() < self.policy.max_evicts_per_frame as usize;
            let upload_fits = plan.bytes_planned.saturating_add(candidate.bytes)
                <= self.policy.max_upload_bytes_per_frame;

            match self.resident.get_mut(&candidate.region_id) {
                Some(state) if state.reuse_key.compatible_with(&reuse_key) => {
                    state.last_touched = tick;
                    plan.unchanged.push(candidate.region_id.clone());
                }
                Some(_) if admit_budget_remaining && evict_budget_remaining && upload_fits => {
                    plan.evicts.push(ResidencyEvict {
                        region_id: candidate.region_id.clone(),
                        stale: true,
                    });
                    plan.bytes_planned = plan.bytes_planned.saturating_add(candidate.bytes);
                    plan.admits.push(ResidencyAdmit {
                        region_id: candidate.region_id,
                        reuse_key,
                        bytes: candidate.bytes,
                    });
                }
                None if admit_budget_remaining && upload_fits => {
                    plan.bytes_planned = plan.bytes_planned.saturating_add(candidate.bytes);
                    plan.admits.push(ResidencyAdmit {
                        region_id: candidate.region_id,
                        reuse_key,
                        bytes: candidate.bytes,
                    });
                }
                _ => plan.deferred.push(candidate.region_id),
            }
        }
        let mut evict_candidates = self
            .resident
            .values()
            .filter(|state| !desired_ids.contains(&state.region_id))
            .cloned()
            .collect::<Vec<_>>();
        evict_candidates.sort_by(|a, b| {
            a.last_touched
                .get()
                .cmp(&b.last_touched.get())
                .then_with(|| a.region_id.cmp(&b.region_id))
        });
        let remaining_evict_budget =
            (self.policy.max_evicts_per_frame as usize).saturating_sub(plan.evicts.len());
        for state in evict_candidates.into_iter().take(remaining_evict_budget) {
            plan.evicts.push(ResidencyEvict {
                region_id: state.region_id,
                stale: false,
            });
        }
        plan
    }

    pub fn apply(&mut self, plan: &ResidencyPlan, tick: SimulationTick) -> ResidencyReport {
        for evict in &plan.evicts {
            self.resident.remove(&evict.region_id);
        }
        for admit in &plan.admits {
            self.resident.insert(
                admit.region_id.clone(),
                ResidentRegionState {
                    region_id: admit.region_id.clone(),
                    reuse_key: admit.reuse_key.clone(),
                    resident_since: tick,
                    last_touched: tick,
                    bytes: admit.bytes,
                },
            );
        }
        let resident_region_ids = self.resident.keys().cloned().collect::<Vec<_>>();
        ResidencyReport {
            admitted: plan.admits.len() as u32,
            evicted: plan.evicts.len() as u32,
            unchanged: plan.unchanged.len() as u32,
            deferred: plan.deferred.len() as u32,
            bytes_uploaded: plan.bytes_planned,
            gpu_cache_hits: 0,
            gpu_cache_misses: 0,
            admitted_region_ids: plan
                .admits
                .iter()
                .map(|admit| admit.region_id.clone())
                .collect(),
            evicted_region_ids: plan
                .evicts
                .iter()
                .map(|evict| evict.region_id.clone())
                .collect(),
            resident_region_ids,
        }
    }

    pub fn apply_with_gpu_cache(
        &mut self,
        plan: &ResidencyPlan,
        snapshot: &WorldSnapshotHandle,
        tick: SimulationTick,
        gpu_cache: &mut ResidencyGpuCache,
    ) -> Result<ResidencyReport, ResidencyError> {
        for evict in &plan.evicts {
            if let Some(key) = gpu_cache.keys_by_region.remove(&evict.region_id) {
                gpu_cache.cache.remove(&key);
            }
        }
        let mut hits = 0u32;
        let mut misses = 0u32;
        for admit in &plan.admits {
            let key = ResidencyGpuCache::key_for(snapshot, admit);
            if gpu_cache.cache.get(&key).is_some() {
                hits = hits.saturating_add(1);
            } else {
                misses = misses.saturating_add(1);
                if let Some(reason) = gpu_cache.fail_next_insert.take() {
                    return Err(ResidencyError::GpuCacheInsert(reason));
                }
                let payload = ResidentRegionGpuPayload {
                    region_id: admit.region_id.clone(),
                    bytes: admit.bytes,
                    compatibility_hash: admit.reuse_key.compatibility_hash,
                };
                gpu_cache
                    .cache
                    .get_or_insert_with::<(), _>(key.clone(), |key| {
                        Ok(GpuResidentScene::new(key.clone(), payload))
                    })
                    .map_err(|()| ResidencyError::GpuCacheInsert("unknown".into()))?;
            }
            gpu_cache
                .keys_by_region
                .insert(admit.region_id.clone(), key);
        }
        let mut report = self.apply(plan, tick);
        report.gpu_cache_hits = hits;
        report.gpu_cache_misses = misses;
        Ok(report)
    }
}

fn predicted_position(target: &FollowTarget, prediction_horizon_secs: f32) -> [f32; 3] {
    if let Some(velocity) = target.velocity {
        [
            target.transform.translation[0]
                + velocity.meters_per_second[0] * prediction_horizon_secs,
            target.transform.translation[1]
                + velocity.meters_per_second[1] * prediction_horizon_secs,
            target.transform.translation[2]
                + velocity.meters_per_second[2] * prediction_horizon_secs,
        ]
    } else {
        target.transform.translation
    }
}

fn region_selection_signature(region_id: &RegionId) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    region_id.hash(&mut hasher);
    hasher.finish()
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}
