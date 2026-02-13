# SLO Burn-Rate Alerting (WRE-638)

This document defines page-worthy burn-rate alerts for global DB operations.

## Alert Families

Each family has two windows:

- fast-burn: 5m / 1h window pair, page immediately.
- slow-burn: 30m / 6h window pair, page if sustained.

## Core SLO Signals

1. `availability.error_budget_burn`
- Objective: keep availability within error budget.
- Fast-burn threshold: `burn_rate >= 14.4`
- Slow-burn threshold: `burn_rate >= 6.0`

2. `durability.ack_loss_risk_burn`
- Objective: no acknowledged-write durability contract regressions.
- Fast-burn threshold: `burn_rate >= 10.0`
- Slow-burn threshold: `burn_rate >= 4.0`

3. `latency.read_p99_burn`
- Objective: protect strong-read p99 latency envelope.
- Fast-burn threshold: `burn_rate >= 8.0`
- Slow-burn threshold: `burn_rate >= 3.0`

4. `replication.safe_time_lag_burn`
- Objective: keep safe-time lag and replication lag within budget.
- Fast-burn threshold: `burn_rate >= 8.0`
- Slow-burn threshold: `burn_rate >= 3.0`

5. `residency.egress_deny_spike_burn`
- Objective: detect residency policy unsat/deny spikes quickly.
- Fast-burn threshold: `burn_rate >= 5.0`
- Slow-burn threshold: `burn_rate >= 2.0`

## Alert Routing

- Severity `SEV-1`: fast-burn breach on durability or availability.
- Severity `SEV-2`: fast-burn breach on latency/replication/residency, or slow-burn on durability/availability.
- Severity `SEV-3`: slow-burn on latency/replication/residency.

Primary pager: on-call DB incident commander.
Secondary pager: platform/runtime owner.

## Required Evidence Artifact

Gate report must include `slo-burn-rate-alerts.json` proving:

- alert definitions exist for each signal above,
- window pairs and thresholds are present,
- alert IDs map to the runbook sections in `incident-command-runbook.md`.
