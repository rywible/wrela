# DR RPO/RTO Drill Gate

Artifact path: `artifacts/dr-drill-report.json`

## Contract

- DR drills must emit machine-readable outcomes.
- Gate thresholds:
  - `max_rpo_commits`
  - `max_rto_ms`
- Gate fails if either threshold is exceeded.

## Local Run

```bash
scripts/db-dr/run_drill.py --out artifacts/dr-drill-report.json
```

## CI Entry

```bash
cargo test -p wrela_runtime --test db_dr_drills -- --nocapture
```

The integration test prints a single-line marker with JSON payload:

- `DRILL_REPORT_JSON:{...}`

The runner parses that payload, writes the artifact, and exits non-zero when `overall_pass=false`.

## Artifact Schema

```json
{
  "rpo_commits": 1,
  "rto_ms": 1900,
  "thresholds": {
    "max_rpo_commits": 2,
    "max_rto_ms": 2500
  },
  "rpo_pass": true,
  "rto_pass": true,
  "overall_pass": true,
  "degraded_network": true,
  "partial_failure": true
}
```
