# Tenant Isolation and QoS

## Scope

- Deterministic tenant quota admission controller.
- Explicit typed retry metadata on quota/in-flight rejections.
- Fair scheduler preventing noisy-neighbor starvation.
- Per-tenant cache budget isolation.

## Runtime Surfaces

- `runtime/src/db/tenant/mod.rs`
  - `TenantAdmissionController`
  - `FairScheduler`
  - `CacheIsolationManager`

## Rejection Contract

- `AdmissionToken::UnknownTenant`
- `AdmissionToken::QuotaWindowExceeded`
- `AdmissionToken::InFlightLimitExceeded`

Every rejection carries `retry_after_ms`.

## Verification

```bash
cargo test -p wrela_runtime db::tenant::
cargo test -p wrela_runtime --test db_tenant_qos
```

