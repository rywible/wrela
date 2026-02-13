# Autopilot Replay Runbook

## Deterministic Replay Harness

Input schema (`scripts/db-autopilot-sim/run.py`):

- `scenario_id`
- `max_to_mean_ratio`
- `skew_threshold`
- `survivable_additional_failures`
- `required_additional_failures`
- `degraded_selected`
- `max_degraded_selected`

Run:

```bash
python3 scripts/db-autopilot-sim/run.py \
  --input scripts/db-autopilot-sim/example-input.json \
  --out artifacts/autopilot-replay-report.json
```

Pass/fail contract:

- pass when skew, failure budget, and degraded-node caps all satisfy policy.
- fail closed when any invariant breaches.

Output artifact:

- `artifacts/autopilot-replay-report.json`
- machine-readable scenario status + deterministic timeline rows.
