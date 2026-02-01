# Runtime Production Readiness Checklist (HA)

Focus: runtime correctness + HA behavior (no ops/monitoring items).

## Core Correctness
- [x] Storage CAS + linearizable reads for correctness-sensitive paths.
- [x] Scheduler HA with leader lease + run dedupe.
- [x] Jobs HA with per-job lease + idempotency markers.
- [x] Realtime fanout across nodes (best-effort) + membership persistence.
- [x] Search index persistence + restart consistency.

## HA Failure Modes
- [x] Leader failover handling (scheduler/jobs).
- [x] Lease expiry recovery for jobs.
- [x] Multi-node chaos/burst tests.
- [x] Ignored soak tests for long-running scenarios.
- [x] Partition/partial outage scenarios (replication drop + recovery).

## Security-Related Runtime Gates
- [x] Peer-to-peer auth for intra-cluster HTTP.
- [x] HTTP auth token and JWT gating.
- [x] HTTP RBAC enforcement (storage-backed).
- [x] HTTP rate limiting (storage-backed).
- [x] HTTP security headers (CSP/HSTS/referrer/XFO/XCTO).
- [x] Per-route RBAC and rate-limit skip paths.

## Remaining High-Risk Gaps
- [x] HTTP auth/rbac/rate-limit behavior under storage outages (fail-closed with 503).
- [x] Durable pub/sub retry (storage-backed DLQ + retry loop).
- [ ] Partition simulation for Raft + service endpoints.
