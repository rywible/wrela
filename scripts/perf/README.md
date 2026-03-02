# Perf Scripts (`scripts/perf`)

Owner scope: perf scaffolding + anti-cheat automation only.

## Required Usage

### 1) PR Fly perf gate (pinned SHA)

```bash
scripts/perf/fly_pr_perf_gate.sh --sha <40-char-commit-sha>
```

### 2) Refresh main canonical baseline (pinned SHA)

```bash
scripts/perf/fly_refresh_main_baseline.sh --sha <40-char-main-sha>
```

### 3) Capture local DB-write baseline (pinned SHA)

```bash
scripts/perf/local_db_write_baseline_capture.sh --sha <40-char-commit-sha>
```

Always runs in strict real-quorum mode.

### 4) Assert local DB-write artifact schema

```bash
scripts/perf/assert_local_db_write_schema.sh .artifacts/perf/local-db-write/<run-id>
# or
scripts/perf/assert_local_db_write_schema.sh .artifacts/perf/local-db-write/<run-id>/summary.json
```

### 5) Assert strict local DB-write evidence

```bash
scripts/perf/assert_strict_local_db_write_evidence.sh .artifacts/perf/local-db-write/<run-id>
# or
scripts/perf/assert_strict_local_db_write_evidence.sh .artifacts/perf/local-db-write/<run-id>/summary.json
```

### 6) Strict meso compare (2x strict runs)

```bash
scripts/perf/local_db_write_meso_compare.sh
# optional third strict baseline append:
scripts/perf/local_db_write_meso_compare.sh --with-control
# enforce multi-lane shard spread evidence when lane_count > 1:
scripts/perf/local_db_write_meso_compare.sh --require-lane-spread
```

### 7) Index local DB-write artifacts (anti-cheat evidence ledger)

```bash
scripts/perf/index_local_db_write_artifact.sh .artifacts/perf/local-db-write/<run-id>
# or
scripts/perf/index_local_db_write_artifact.sh .artifacts/perf/local-db-write/<run-id>/summary.json --kind manual
```

The index ledger is appended at:

```text
.artifacts/perf/local-db-write/INDEX.jsonl
```

### 8) Assert local DB-write run is claimable evidence

```bash
scripts/perf/assert_local_db_write_claimable.sh .artifacts/perf/local-db-write/<run-id>
scripts/perf/assert_local_db_write_claimable.sh .artifacts/perf/local-db-write/<run-id>/summary.json --strict-required 1 --require-indexed 1
scripts/perf/assert_local_db_write_claimable.sh .artifacts/perf/local-db-write/<run-id>/summary.json --strict-required 1 --require-indexed 1 --require-lane-spread 1
```

`local_db_write_baseline_capture.sh` also supports `WRELA_LOCAL_PERF_REQUIRE_LANE_SPREAD=1` to enforce the same lane-spread claimability check during baseline promotion.

### 9) Strict profile sweep (anti-cheat gated)

```bash
scripts/perf/local_db_write_profile_sweep.sh
# custom short probe:
scripts/perf/local_db_write_profile_sweep.sh --duration 2 --payload 64 \
  --profiles "base:8:2:8:500:64:262144:1:25,wide:12:4:16:800:128:524288:1:25" \
  --require-lane-spread 1
```

Artifacts are emitted under:

```text
.artifacts/perf/local-db-write/sweeps/<run-id>.json
```

### 10) Jupiter integration blocking matrix (full gate)

```bash
scripts/perf/jupiter_integration_blocking_matrix.sh --sha <40-char-commit-sha>
# local-only debug run (not valid as final gate evidence):
scripts/perf/jupiter_integration_blocking_matrix.sh --sha <40-char-commit-sha> --skip-fly
```

## Fly Pool Config Requirement

`fly_pr_perf_gate.sh` and `fly_refresh_main_baseline.sh` require `scripts/perf/fly_pool.json`.
Start from:

```bash
cp scripts/perf/fly_pool.example.json scripts/perf/fly_pool.json
```

Expected minimum shape:

```json
{
  "amd64": {
    "name": "wrela-perf-runner-1",
    "app": "wrela-perf-runner-1",
    "machine_id": "78469e2c2e0518",
    "region": "iad"
  }
}
```

## Anti-Cheat Policy (Fail-Closed)

- Pinned SHA is mandatory and must resolve to a full 40-char commit.
- SHA must be fetchable from `origin` and contained in an `origin/*` ref.
- Dirty local trees are rejected for baseline/gate runs.
- Fly gates enforce fixed suite set: `micro meso macro linux`.
- Fly gate defaults enforce evidence-grade runs: `PERF_RUNS>=10`, `PERF_WARMUP_RUNS>=1`.
- Candidate CV is fail-closed by `PERF_CV_MAX_PCT` (default `10`).
- Missing canonical baseline, missing runner config, missing artifacts, or schema drift all fail immediately.
- Local DB-write schema checks explicitly enforce:
  - `p99_ms` and `p999_ms`
  - `replication.queue_depth`
  - `client_write_path.response_wait_pct`
- Strict local evidence checks enforce:
  - `config.real_quorum_mode == true`
  - `replication.real_quorum_evidence == true`
  - `replication.simulation_commits == 0`
