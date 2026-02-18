# SQL Conformance and Throughput Gates

This gate tracks SQL subset correctness and reproducible benchmark artifacts.

## Correctness

Conformance cases validate parser/catalog/planner expectations, including reject paths.

Command:

```bash
cargo test -p wrela_runtime --test db_sql_conformance -- --nocapture
```

## Throughput Artifact

Command:

```bash
python3 scripts/db-bench/sql/run_sql_bench.py --out artifacts/sql-bench-report.json
```

Artifact contract:

- `version`
- `total_ops`
- `weighted_avg_latency_ms`
- per-statement rows (`insert`, `point_select`, `range_scan`, `update`, `delete`)
