use wrela_runtime::db::tenant::{
    AdmissionToken, CacheIsolationManager, FairScheduler, TenantAdmissionController, TenantQuota,
    WorkClass, WorkItem,
};

fn quota(weight: u32, cache_bytes_quota: u64) -> TenantQuota {
    TenantQuota {
        max_in_flight: 2,
        max_ops_per_window: 4,
        window_ms: 100,
        retry_after_ms: 25,
        weight,
        cache_bytes_quota,
    }
}

#[test]
fn tenant_quota_admission_is_deterministic_with_retry_metadata() {
    let mut admission = TenantAdmissionController::default();
    admission
        .register_tenant("tenant-a", quota(1, 128))
        .expect("register");

    let p1 = admission
        .admit("tenant-a", WorkClass::Read, 1_000)
        .expect("permit1");
    let _p2 = admission
        .admit("tenant-a", WorkClass::Write, 1_001)
        .expect("permit2");

    let reject = admission
        .admit("tenant-a", WorkClass::Scan, 1_002)
        .expect_err("should reject third in-flight");
    assert_eq!(reject.token, AdmissionToken::InFlightLimitExceeded);
    assert_eq!(reject.retry_after_ms, 25);

    admission.release(&p1);
    let _p3 = admission
        .admit("tenant-a", WorkClass::Scan, 1_003)
        .expect("permit3");
    let stats = admission.stats("tenant-a").expect("stats");
    assert_eq!(stats.admitted, 3);
    assert_eq!(stats.throttled, 1);
}

#[test]
fn fairness_and_cache_isolation_hold_under_noisy_neighbor_pressure() {
    let q_a = quota(1, 64);
    let q_b = quota(1, 64);

    let mut cache = CacheIsolationManager::default();
    assert!(cache.can_charge("tenant-a", 64, &q_a));
    cache.charge("tenant-a", 64);
    assert!(
        !cache.can_charge("tenant-a", 1, &q_a),
        "tenant-a must not exceed own cache quota"
    );
    assert!(
        cache.can_charge("tenant-b", 64, &q_b),
        "tenant-b cache budget must remain independent"
    );

    let mut scheduler = FairScheduler::default();
    scheduler.register_tenant("tenant-a", &q_a);
    scheduler.register_tenant("tenant-b", &q_b);
    for i in 0..32 {
        scheduler.submit(WorkItem {
            tenant: "tenant-a".to_string(),
            id: i,
            class: WorkClass::Read,
            bytes: 8,
        });
    }
    scheduler.submit(WorkItem {
        tenant: "tenant-b".to_string(),
        id: 9_999,
        class: WorkClass::Read,
        bytes: 8,
    });

    let mut served_b = false;
    for _ in 0..8 {
        let item = scheduler.dispatch_next().expect("dispatch");
        if item.tenant == "tenant-b" {
            served_b = true;
            break;
        }
    }
    assert!(served_b, "fair scheduler must prevent starvation");
}
