use crate::db::time::hlc::HlTimestamp;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SafeTimeLagBudget {
    pub shard_lag_ms: u64,
    pub region_lag_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LagViolation {
    pub scope: &'static str,
    pub id: String,
    pub lag_ms: u64,
    pub budget_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafeTimeDiagnostics {
    pub global_safe_time: Option<u64>,
    pub region_safe_times: BTreeMap<String, u64>,
    pub shard_safe_times: BTreeMap<String, u64>,
    pub violations: Vec<LagViolation>,
}

#[derive(Debug, Default)]
pub struct SafeTimePropagator {
    shard_safe_times: BTreeMap<String, u64>,
    region_safe_times: BTreeMap<String, u64>,
    region_shards: BTreeMap<String, BTreeSet<String>>,
}

impl SafeTimePropagator {
    pub fn observe_shard_safe_time(
        &mut self,
        shard_id: impl Into<String>,
        region: impl Into<String>,
        safe_ts_packed: u64,
    ) {
        let shard_id = shard_id.into();
        let region = region.into();
        let entry = self.shard_safe_times.entry(shard_id.clone()).or_insert(0);
        *entry = (*entry).max(safe_ts_packed);
        self.region_shards
            .entry(region)
            .or_default()
            .insert(shard_id);
        self.recompute_region_safe_times();
    }

    /// Updates the shard entry without triggering a full region recomputation.
    /// Call `recompute_region_safe_times()` once after a batch of updates.
    pub fn observe_shard_safe_time_no_recompute(
        &mut self,
        shard_id: impl Into<String>,
        region: impl Into<String>,
        safe_ts_packed: u64,
    ) {
        let shard_id = shard_id.into();
        let region = region.into();
        let entry = self.shard_safe_times.entry(shard_id.clone()).or_insert(0);
        *entry = (*entry).max(safe_ts_packed);
        self.region_shards
            .entry(region)
            .or_default()
            .insert(shard_id);
    }

    pub fn recompute_region_safe_times(&mut self) {
        self.region_safe_times.clear();
        for (region, shards) in &self.region_shards {
            let min_safe = shards
                .iter()
                .filter_map(|shard| self.shard_safe_times.get(shard).copied())
                .min();
            if let Some(min_safe) = min_safe {
                self.region_safe_times.insert(region.clone(), min_safe);
            }
        }
    }

    pub fn region_safe_time(&self, region: &str) -> Option<u64> {
        self.region_safe_times.get(region).copied()
    }

    pub fn global_safe_time(&self) -> Option<u64> {
        self.region_safe_times.values().copied().min()
    }

    pub fn diagnostics(&self, now_packed: u64, budgets: SafeTimeLagBudget) -> SafeTimeDiagnostics {
        let now = HlTimestamp::unpack(now_packed);
        let mut violations = Vec::new();

        for (region, safe_ts) in &self.region_safe_times {
            let lag_ms = now
                .physical_ms
                .saturating_sub(HlTimestamp::unpack(*safe_ts).physical_ms);
            if lag_ms > budgets.region_lag_ms {
                violations.push(LagViolation {
                    scope: "region",
                    id: region.clone(),
                    lag_ms,
                    budget_ms: budgets.region_lag_ms,
                });
            }
        }

        for (shard, safe_ts) in &self.shard_safe_times {
            let lag_ms = now
                .physical_ms
                .saturating_sub(HlTimestamp::unpack(*safe_ts).physical_ms);
            if lag_ms > budgets.shard_lag_ms {
                violations.push(LagViolation {
                    scope: "shard",
                    id: shard.clone(),
                    lag_ms,
                    budget_ms: budgets.shard_lag_ms,
                });
            }
        }

        violations.sort_by(|a, b| {
            a.scope
                .cmp(b.scope)
                .then_with(|| a.id.cmp(&b.id))
                .then_with(|| a.lag_ms.cmp(&b.lag_ms))
        });

        SafeTimeDiagnostics {
            global_safe_time: self.global_safe_time(),
            region_safe_times: self.region_safe_times.clone(),
            shard_safe_times: self.shard_safe_times.clone(),
            violations,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn propagation_converges_to_region_and_global_minima() {
        let mut p = SafeTimePropagator::default();
        p.observe_shard_safe_time(
            "s-a",
            "us",
            HlTimestamp {
                physical_ms: 100,
                logical: 0,
            }
            .pack(),
        );
        p.observe_shard_safe_time(
            "s-b",
            "us",
            HlTimestamp {
                physical_ms: 90,
                logical: 0,
            }
            .pack(),
        );
        p.observe_shard_safe_time(
            "s-c",
            "eu",
            HlTimestamp {
                physical_ms: 110,
                logical: 0,
            }
            .pack(),
        );
        p.recompute_region_safe_times();

        assert_eq!(
            p.region_safe_time("us"),
            Some(
                HlTimestamp {
                    physical_ms: 90,
                    logical: 0
                }
                .pack()
            )
        );
        assert_eq!(
            p.global_safe_time(),
            Some(
                HlTimestamp {
                    physical_ms: 90,
                    logical: 0
                }
                .pack()
            )
        );
    }

    #[test]
    fn diagnostics_reports_lag_budget_violations_deterministically() {
        let mut p = SafeTimePropagator::default();
        p.observe_shard_safe_time(
            "s-1",
            "us",
            HlTimestamp {
                physical_ms: 50,
                logical: 0,
            }
            .pack(),
        );
        p.observe_shard_safe_time(
            "s-2",
            "eu",
            HlTimestamp {
                physical_ms: 80,
                logical: 0,
            }
            .pack(),
        );
        p.recompute_region_safe_times();
        let diag = p.diagnostics(
            HlTimestamp {
                physical_ms: 120,
                logical: 0,
            }
            .pack(),
            SafeTimeLagBudget {
                shard_lag_ms: 30,
                region_lag_ms: 40,
            },
        );
        assert_eq!(diag.violations.len(), 3);
        assert_eq!(diag.violations[0].scope, "region");
        assert_eq!(diag.violations[0].id, "us");
        assert_eq!(diag.violations[1].scope, "shard");
        assert_eq!(diag.violations[1].id, "s-1");
    }

    #[test]
    fn observe_keeps_region_and_global_safe_time_fresh_without_manual_recompute() {
        let mut p = SafeTimePropagator::default();
        p.observe_shard_safe_time(
            "s-a",
            "us",
            HlTimestamp {
                physical_ms: 120,
                logical: 0,
            }
            .pack(),
        );
        p.observe_shard_safe_time(
            "s-b",
            "us",
            HlTimestamp {
                physical_ms: 110,
                logical: 0,
            }
            .pack(),
        );
        p.observe_shard_safe_time(
            "s-c",
            "eu",
            HlTimestamp {
                physical_ms: 130,
                logical: 0,
            }
            .pack(),
        );

        assert_eq!(
            p.region_safe_time("us"),
            Some(
                HlTimestamp {
                    physical_ms: 110,
                    logical: 0
                }
                .pack()
            )
        );
        assert_eq!(
            p.global_safe_time(),
            Some(
                HlTimestamp {
                    physical_ms: 110,
                    logical: 0
                }
                .pack()
            )
        );
    }
}
