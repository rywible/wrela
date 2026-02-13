# Canonical Overlay DAG (P10–P13 and X-Lanes)

Reference dependencies for the overlay/control plane phases only.
This file is the source of truth for issues owned by G1-4 (`WRE-637`).

## Governance Control Nodes

- `WRE-603` (G1-1 weekly drift checker) depends on:
  - `WRE-592` (G1 umbrella)
- `WRE-610` (G1-2 completeness backfill) depends on:
  - `WRE-592` (G1 umbrella)
  - `WRE-611` (P0-9 dependency hygiene prerequisite)

## Overlay Phase Order

- `WRE-612` (Phase 10: Deterministic Replay) depends on:
  - `WRE-593` (G2 gates)
  - `WRE-461`
  - `WRE-453`
  - `WRE-592`
  - `WRE-454`
- `WRE-613` (Phase 11: CDC) depends on:
  - `WRE-497` (Policy autopilot)
  - `WRE-461` (consensus core)
  - `WRE-489` (distributed SQL DML/index maintenance)
  - `WRE-470` (typed edge API semantics)
  - `WRE-592`
- `WRE-614` (Phase 12: Advisor/Evolution) depends on:
  - `WRE-589` (X4)
  - `WRE-590` (X5)
  - `WRE-612` (deterministic replay prerequisites)
  - `WRE-592`
  - `WRE-449`
  - `WRE-479` (MRA)
- `WRE-627` (Phase 13: Analytics Plane) depends on:
  - `WRE-613` (CDC)
  - `WRE-491` (backup/restore)
  - `WRE-497` (policy autopilot)
  - `WRE-614` (advisor/evolution)
  - `WRE-592`

## X-Lane Pre-requisites (as currently encoded)

- `WRE-587` (X2 Transaction Lifecycle) depends on:
  - `WRE-473` (Phase 4 transactions + HLC)
  - `WRE-592`
- `WRE-588` (X3 Safe-time) depends on:
  - `WRE-473`
  - `WRE-592`
- `WRE-589` (X4 Schema evolution) depends on:
  - `WRE-592`
- `WRE-590` (X5 Statistics/Cost) depends on:
  - `WRE-592`
  - `WRE-485` (SQL surface)
- `WRE-591` (X6 Tenant QoS) depends on:
  - `WRE-607` (tenant fairness)
  - `WRE-592`

## Notes

- `WRE-613` (Phase 11 CDC) is an upstream hard prerequisite for both
  `WRE-627` and `WRE-628` (+ follow-on CDC ingestion child work).
- `WRE-627` should not be started before `WRE-614` unless a migration plan is
  explicitly documented as an exception.
- The governance checker in `scripts/governance/check_g1_governance.py` must
  remain in lockstep with this document's canonical blocker edges.
