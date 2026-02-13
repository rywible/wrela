# Performance Gate

## Lanes

- Soak lane (deterministic synthetic): `scripts/db-soak/run_soak.py`
- Regression lane: `scripts/db-bench/regression_gate/check_regression.py`

## Commands

```bash
scripts/db-soak/run_soak.py --duration-s 300 --target-ops-per-s 5000 --threshold-p99-ms 2.0 --seed 7 --phase phase-9 --environment ci-pr
scripts/db-bench/regression_gate/check_regression.py --baseline-tps 10000 --current-tps 9700 --baseline-p99-ms 2.0 --current-p99-ms 2.2 --max-drop-pct 5.0 --max-tail-increase-pct 15.0 --baseline-phase phase-8 --current-phase phase-9
```

## Blocking Policy

- Soak lane fails when `p99_ms > threshold_p99_ms`.
- Soak lane also fails on invalid metadata (`environment` outside `ci-pr|ci-nightly|local`).
- Regression lane fails when throughput regression exceeds `max_drop_pct`.
- Regression lane fails independently when p99 tail-latency increase exceeds `max_tail_increase_pct`.
- Both lanes are release blockers for Phase 9.

## Report Contract

- Soak output includes deterministic lineage fields: `phase`, `seed`, `duration_s`, `target_ops_per_s`.
- Regression output includes `lineage.baseline_phase` and `lineage.current_phase`.
- Tail-latency and throughput outcomes are split as:
  - `throughput_passed`
  - `tail_latency_passed`
