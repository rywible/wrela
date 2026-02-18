# DB Perf Baseline

Current baseline collection target:

- single-node write-heavy lane
- single-node mixed read/write lane
- replayable seed and machine metadata

Artifacts should be emitted into `artifacts/perf-baseline.json` with:

- ops/sec
- p95 and p99 latency
- batch size configuration
- WAL fsync timing summary

