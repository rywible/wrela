# Incident Command Runbook (WRE-638)

This runbook is the operator map for page-triggering global DB incidents.

## IC Quick Start

1. Declare incident severity (`SEV-1`/`SEV-2`/`SEV-3`).
2. Assign roles:
- Incident Commander (IC)
- Operations Lead
- Communications Lead
- Scribe
3. Lock in timeline and decision log.
4. Link active alerts and affected regions/tenants.

## Alert to Runbook Mapping

| Alert Family | Primary Section |
| --- | --- |
| availability.error_budget_burn | Regional Outage |
| durability.ack_loss_risk_burn | Split-Brain Suspicion |
| latency.read_p99_burn | CDC Backlog Runaway |
| replication.safe_time_lag_burn | Snapshot/Restore Degradation |
| residency.egress_deny_spike_burn | Policy Unsatisfiable Storm |
| metadata.authority_failover_burn | Metadata Authority Failover |

## Regional Outage

1. Verify quorum/safe-time status by region.
2. Trigger failover orchestrator only if policy satisfiable.
3. Freeze non-essential topology changes.
4. Collect evidence:
- pre/post failover routing decisions
- quorum health snapshots
- recovered read/write SLO snapshots

## Split-Brain Suspicion

1. Halt unsafe writes on conflicting leaders.
2. Capture leader term/index state from all replicas.
3. Enforce single-authority election path.
4. Collect evidence:
- term/index divergence report
- conflict resolution timeline
- post-recovery invariant checks

## Policy Unsatisfiable Storm

1. Confirm deny token mix (`RESIDENCY_EGRESS_POLICY_UNSAT`, related deny tokens).
2. Run topology/profile preflight solver.
3. Apply only satisfiable policy/topology changes.
4. Collect evidence:
- unsat cause analysis
- candidate plan evaluation output
- accepted plan and post-change deny-rate

## CDC Backlog Runaway

1. Check checkpoint/resume lag by stream.
2. Separate sink slowness from source ordering issues.
3. Run CDC correctness gate on sampled API pages (must stay monotonic and cursor-safe).
4. Run CDC perf gate with current stream metrics:
- throughput ratio (`sink/source`) must stay above floor.
- backlog events must stay below cap.
- replay lag seconds must stay below cap.
5. Apply stream backpressure and recovery playbook.
4. Collect evidence:
- checkpoint growth curves
- sink throughput vs source emit rate
- resumed stream integrity checks
- correctness/perf gate report rows and threshold config

## Metadata Authority Failover

1. Confirm current authority node is unavailable (health + control-plane lease check).
2. Execute metadata authority failover to the next healthy voter outside drained regions.
3. Verify authority epoch increment and trace output (`plan` then `apply`).
4. If replacement requires forced movement, execute rebootstrap with current epoch token.
5. Reject stale rebootstrap attempts; require fresh epoch from latest state.
6. Collect evidence:
- pre-failover authority node + epoch
- failover decision trace
- post-failover authority node + epoch
- any rejected stale rebootstrap attempts

## Snapshot/Restore Degradation

1. Validate snapshot manifest/checksum first.
2. Check replay tail progress and safe-time convergence.
3. Trigger controlled restore fallback if RTO at risk.
4. Collect evidence:
- restore stage durations
- replay progress and lag
- final RPO/RTO outcome vs target

## Drill Artifact Contract

Every major incident class requires at least one simulation drill artifact entry in
`incident-drill-artifacts.json` including:

- incident class
- exercise date
- scenario seed
- outcome
- follow-up actions
