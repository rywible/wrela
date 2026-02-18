use std::collections::{BTreeMap, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkClass {
    Read,
    Write,
    Scan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantQuota {
    pub max_in_flight: usize,
    pub max_ops_per_window: u64,
    pub window_ms: u64,
    pub retry_after_ms: u64,
    pub weight: u32,
    pub cache_bytes_quota: u64,
}

impl TenantQuota {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.max_in_flight == 0 {
            return Err("max_in_flight must be > 0");
        }
        if self.max_ops_per_window == 0 {
            return Err("max_ops_per_window must be > 0");
        }
        if self.window_ms == 0 {
            return Err("window_ms must be > 0");
        }
        if self.retry_after_ms == 0 {
            return Err("retry_after_ms must be > 0");
        }
        if self.weight == 0 {
            return Err("weight must be > 0");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionToken {
    UnknownTenant,
    QuotaWindowExceeded,
    InFlightLimitExceeded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmissionReject {
    pub token: AdmissionToken,
    pub retry_after_ms: u64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmissionPermit {
    tenant: String,
    pub class: WorkClass,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantAdmissionStats {
    pub in_flight: usize,
    pub window_ops: u64,
    pub admitted: u64,
    pub throttled: u64,
}

#[derive(Debug, Clone)]
struct TenantRuntime {
    quota: TenantQuota,
    in_flight: usize,
    window_start_ms: u64,
    window_ops: u64,
    admitted: u64,
    throttled: u64,
}

impl TenantRuntime {
    fn new(quota: TenantQuota) -> Self {
        Self {
            quota,
            in_flight: 0,
            window_start_ms: 0,
            window_ops: 0,
            admitted: 0,
            throttled: 0,
        }
    }

    fn refresh_window(&mut self, now_ms: u64) {
        if self.window_start_ms == 0 {
            self.window_start_ms = now_ms;
            return;
        }
        if now_ms.saturating_sub(self.window_start_ms) >= self.quota.window_ms {
            self.window_start_ms = now_ms;
            self.window_ops = 0;
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TenantAdmissionController {
    tenants: BTreeMap<String, TenantRuntime>,
}

impl TenantAdmissionController {
    pub fn register_tenant(
        &mut self,
        tenant: impl Into<String>,
        quota: TenantQuota,
    ) -> Result<(), &'static str> {
        quota.validate()?;
        self.tenants
            .insert(tenant.into(), TenantRuntime::new(quota));
        Ok(())
    }

    pub fn admit(
        &mut self,
        tenant: &str,
        class: WorkClass,
        now_ms: u64,
    ) -> Result<AdmissionPermit, AdmissionReject> {
        let Some(runtime) = self.tenants.get_mut(tenant) else {
            return Err(AdmissionReject {
                token: AdmissionToken::UnknownTenant,
                retry_after_ms: 1_000,
                reason: format!("unknown tenant: {tenant}"),
            });
        };
        runtime.refresh_window(now_ms);
        if runtime.in_flight >= runtime.quota.max_in_flight {
            runtime.throttled = runtime.throttled.saturating_add(1);
            return Err(AdmissionReject {
                token: AdmissionToken::InFlightLimitExceeded,
                retry_after_ms: runtime.quota.retry_after_ms,
                reason: format!(
                    "tenant={tenant} class={class:?} exceeded in-flight limit {}",
                    runtime.quota.max_in_flight
                ),
            });
        }
        if runtime.window_ops >= runtime.quota.max_ops_per_window {
            runtime.throttled = runtime.throttled.saturating_add(1);
            return Err(AdmissionReject {
                token: AdmissionToken::QuotaWindowExceeded,
                retry_after_ms: runtime.quota.retry_after_ms,
                reason: format!(
                    "tenant={tenant} class={class:?} exceeded window quota {}",
                    runtime.quota.max_ops_per_window
                ),
            });
        }

        runtime.in_flight = runtime.in_flight.saturating_add(1);
        runtime.window_ops = runtime.window_ops.saturating_add(1);
        runtime.admitted = runtime.admitted.saturating_add(1);
        Ok(AdmissionPermit {
            tenant: tenant.to_string(),
            class,
        })
    }

    pub fn release(&mut self, permit: &AdmissionPermit) {
        if let Some(runtime) = self.tenants.get_mut(&permit.tenant) {
            runtime.in_flight = runtime.in_flight.saturating_sub(1);
        }
    }

    pub fn stats(&self, tenant: &str) -> Option<TenantAdmissionStats> {
        self.tenants
            .get(tenant)
            .map(|runtime| TenantAdmissionStats {
                in_flight: runtime.in_flight,
                window_ops: runtime.window_ops,
                admitted: runtime.admitted,
                throttled: runtime.throttled,
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkItem {
    pub tenant: String,
    pub id: u64,
    pub class: WorkClass,
    pub bytes: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FairnessStats {
    pub dispatched: u64,
    pub throttled: u64,
    pub starvation_events: u64,
}

#[derive(Debug, Clone)]
struct QueueState {
    queue: VecDeque<WorkItem>,
    served_weighted_units: u64,
    last_dispatch_tick: u64,
}

impl QueueState {
    fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            served_weighted_units: 0,
            last_dispatch_tick: 0,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CacheIsolationManager {
    used_by_tenant: BTreeMap<String, u64>,
}

impl CacheIsolationManager {
    pub fn can_charge(&self, tenant: &str, bytes: u64, quota: &TenantQuota) -> bool {
        let used = self.used_by_tenant.get(tenant).copied().unwrap_or(0);
        used.saturating_add(bytes) <= quota.cache_bytes_quota
    }

    pub fn charge(&mut self, tenant: &str, bytes: u64) {
        let entry = self.used_by_tenant.entry(tenant.to_string()).or_insert(0);
        *entry = entry.saturating_add(bytes);
    }

    pub fn release(&mut self, tenant: &str, bytes: u64) {
        if let Some(used) = self.used_by_tenant.get_mut(tenant) {
            *used = used.saturating_sub(bytes);
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FairScheduler {
    queues: BTreeMap<String, QueueState>,
    weights: BTreeMap<String, u32>,
    tick: u64,
    pub stats: FairnessStats,
}

impl FairScheduler {
    pub fn register_tenant(&mut self, tenant: impl Into<String>, quota: &TenantQuota) {
        let tenant = tenant.into();
        self.queues
            .entry(tenant.clone())
            .or_insert_with(QueueState::new);
        self.weights.insert(tenant, quota.weight);
    }

    pub fn submit(&mut self, item: WorkItem) {
        self.queues
            .entry(item.tenant.clone())
            .or_insert_with(QueueState::new)
            .queue
            .push_back(item);
    }

    pub fn dispatch_next(&mut self) -> Option<WorkItem> {
        self.tick = self.tick.saturating_add(1);
        let mut best: Option<(String, u64, u64)> = None;

        for (tenant, state) in &self.queues {
            if state.queue.is_empty() {
                continue;
            }
            let weight = self.weights.get(tenant).copied().unwrap_or(1) as u64;
            let score = state.served_weighted_units / weight.max(1);
            let tie_breaker = state.last_dispatch_tick;
            match &best {
                None => best = Some((tenant.clone(), score, tie_breaker)),
                Some((_best_tenant, best_score, best_tick)) => {
                    if score < *best_score || (score == *best_score && tie_breaker < *best_tick) {
                        best = Some((tenant.clone(), score, tie_breaker));
                    }
                }
            }
        }

        let (tenant, _, _) = best?;
        let state = self
            .queues
            .get_mut(&tenant)
            .expect("tenant queue must exist");
        let item = state.queue.pop_front()?;
        let weight = self.weights.get(&tenant).copied().unwrap_or(1) as u64;
        state.served_weighted_units = state.served_weighted_units.saturating_add(weight.max(1));
        state.last_dispatch_tick = self.tick;
        self.stats.dispatched = self.stats.dispatched.saturating_add(1);
        Some(item)
    }

    pub fn detect_starvation(&mut self, max_lag_ticks: u64) -> Vec<String> {
        let mut starving = Vec::new();
        for (tenant, state) in &self.queues {
            if state.queue.is_empty() {
                continue;
            }
            if self.tick.saturating_sub(state.last_dispatch_tick) > max_lag_ticks {
                starving.push(tenant.clone());
                self.stats.starvation_events = self.stats.starvation_events.saturating_add(1);
            }
        }
        starving
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AdmissionToken, FairScheduler, TenantAdmissionController, TenantQuota, WorkClass, WorkItem,
    };

    fn quota() -> TenantQuota {
        TenantQuota {
            max_in_flight: 2,
            max_ops_per_window: 3,
            window_ms: 100,
            retry_after_ms: 25,
            weight: 1,
            cache_bytes_quota: 128,
        }
    }

    #[test]
    fn admission_controller_returns_typed_retry_metadata() {
        let mut admission = TenantAdmissionController::default();
        admission
            .register_tenant("tenant-a", quota())
            .expect("register");

        let p1 = admission
            .admit("tenant-a", WorkClass::Read, 1_000)
            .expect("permit 1");
        let p2 = admission
            .admit("tenant-a", WorkClass::Write, 1_001)
            .expect("permit 2");
        let err = admission
            .admit("tenant-a", WorkClass::Scan, 1_002)
            .expect_err("must throttle in-flight");
        assert_eq!(err.token, AdmissionToken::InFlightLimitExceeded);
        assert_eq!(err.retry_after_ms, 25);

        admission.release(&p1);
        admission.release(&p2);
        let _p3 = admission
            .admit("tenant-a", WorkClass::Scan, 1_003)
            .expect("permit 3");
        let err = admission
            .admit("tenant-a", WorkClass::Read, 1_004)
            .expect_err("must throttle by window quota");
        assert_eq!(err.token, AdmissionToken::QuotaWindowExceeded);
    }

    #[test]
    fn fairness_scheduler_prevents_noisy_neighbor_starvation() {
        let mut scheduler = FairScheduler::default();
        scheduler.register_tenant("tenant-a", &quota());
        scheduler.register_tenant("tenant-b", &quota());

        for i in 0..20 {
            scheduler.submit(WorkItem {
                tenant: "tenant-a".to_string(),
                id: i,
                class: WorkClass::Read,
                bytes: 8,
            });
        }
        scheduler.submit(WorkItem {
            tenant: "tenant-b".to_string(),
            id: 1_000,
            class: WorkClass::Read,
            bytes: 8,
        });

        let mut seen_b = false;
        for _ in 0..6 {
            let item = scheduler.dispatch_next().expect("must dispatch");
            if item.tenant == "tenant-b" {
                seen_b = true;
                break;
            }
        }
        assert!(seen_b, "tenant-b must receive service under noisy load");
        assert!(scheduler.detect_starvation(10).is_empty());
    }
}
