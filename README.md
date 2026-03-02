# Wrela

## Compiler Tooling Surface

- `wrela fix` and `wrela fmt` now scope diagnostics/fixes to the requested target path by default.
- Use `--workspace-diagnostics` with `fix`/`fmt` to opt into imported-module diagnostics/fixes.
- `--error-format` is the only diagnostic format selector (`--format` was removed).
- Legacy syntax (`given`, removed `otherwise` forms, and removed keyword variants) is parse-error-only with canonical guidance; `fix`/`fmt` no longer perform migration rewrites for those forms.

## Local Cluster Baseline

Wrela includes a local 3-node cluster regression suite in `runtime/tests/` intended to catch mesh/forwarding/runtime-stability issues before Fly smoke runs, but these tests are currently ignored because `apps/wreladb-lab` was removed.

Core local-cluster tests:

- `db_local_cluster_smoke` (ignored)
- `db_local_cluster_rolling` (ignored)
- `db_local_cluster_runtime_stability` (ignored)

These are not part of normal Rust test execution right now. If the `wreladb-lab` dependency is restored, run them explicitly with ignored tests enabled:

```bash
cargo test -p wrela_runtime \
  --test db_local_cluster_smoke \
  --test db_local_cluster_rolling \
  --test db_local_cluster_runtime_stability \
  -- --ignored
```

For manual debugging outside Rust tests, use:

```bash
scripts/local/wrela_cluster_smoke.sh
```

It writes artifacts under `artifacts/local/`.

## Local DB Write Perf Harness

A dedicated local perf harness is available for write-throughput profiling with JSON artifacts:

```bash
cargo test -p wrela_runtime --test db_write_local_perf \
  -- --ignored --nocapture
```

It emits artifacts under:

```text
.artifacts/perf/local-db-write/<run-id>/
```

Workloads included:

- `raw_write_leader_local`
- `raw_write_round_robin_nodes`
- `validated_write_path`

Common tuning envs:

```bash
WRELA_LOCAL_PERF_DURATION_SECONDS=20 \
WRELA_LOCAL_PERF_CONCURRENCY=32 \
WRELA_LOCAL_PERF_PAYLOAD_BYTES=128 \
cargo test -p wrela_runtime --test db_write_local_perf -- --ignored --nocapture
```

DB write-path tuning envs:

```bash
WRELADB_WRITE_FLUSH_WINDOW_MS=2
WRELADB_WRITE_FLUSH_MAX_OPS=256
WRELADB_WRITE_FLUSH_SOFT_BYTES=524288
WRELADB_CLOCK_PERSIST_INTERVAL_OPS=16
WRELADB_RAFT_PERSIST_INTERVAL_OPS=16
WRELADB_WAL_GROUP_COMMIT_WINDOW_US=1000
WRELADB_WAL_GROUP_COMMIT_MAX_OPS=2048
WRELADB_WAL_GROUP_COMMIT_MAX_BYTES=4194304
WRELADB_WAL_SEGMENT_PREALLOCATE_BYTES=67108864
WRELADB_WAL_WRITEV_ENABLED=1
```

Jupiter full-send runtime defaults (always-on, no per-feature env toggles):

```bash
# Hard defaults in runtime:
# - replication outside lock: enabled
# - WAL encode outside lock: enabled
# - sorted-run catch-up: enabled
# - sorted-run lag threshold: 4096 ops
# - sorted-run chunk sizing: 256 entries / 262144 bytes
# - compaction scheduler: enabled (max debt 134217728 bytes)
# - value separation + blob GC: enabled (threshold 4096 bytes)
# - authorized insert fast lane: enabled (eligibility-guarded)
# - latency frontier planning: enabled
# - replicated log backend default: CanonicalOnly
# Backends DualWal/ShadowCanonical remain available via DbOpenOptions
# for parity/rollback validation only.
```

High-throughput local profile (strict durability + bounded group commit):

```bash
WRELADB_WRITE_FLUSH_WINDOW_MS=1 \
WRELADB_WRITE_FLUSH_MAX_OPS=512 \
WRELADB_WRITE_FLUSH_SOFT_BYTES=1048576 \
WRELADB_CLOCK_PERSIST_INTERVAL_OPS=10000 \
WRELADB_RAFT_PERSIST_INTERVAL_OPS=10000 \
WRELADB_WAL_GROUP_COMMIT_WINDOW_US=1000 \
WRELADB_WAL_GROUP_COMMIT_MAX_OPS=2048 \
WRELADB_WAL_GROUP_COMMIT_MAX_BYTES=4194304 \
WRELADB_WAL_SEGMENT_PREALLOCATE_BYTES=67108864 \
WRELADB_WAL_WRITEV_ENABLED=1 \
WRELA_LOCAL_PERF_DURATION_SECONDS=6 \
WRELA_LOCAL_PERF_CONCURRENCY=32 \
WRELA_LOCAL_PERF_PAYLOAD_BYTES=128 \
cargo test -p wrela_runtime --test db_write_local_perf -- --ignored --nocapture
```

Jupiter full-send blocking matrix (integration gate):

```bash
scripts/perf/jupiter_integration_blocking_matrix.sh --sha "$(git rev-parse HEAD)"
# local-only debug run without Fly drills (not valid for final gate):
scripts/perf/jupiter_integration_blocking_matrix.sh --sha "$(git rev-parse HEAD)" --skip-fly
```

Latest full-gate evidence snapshot:
1. SHA: `68108003d298959ae0be2848c1fd82d5e37e0495`
2. Fly smoke: `artifacts/fly/wrela-smoke-20260224121731-smoke-report.json`
3. Fly write load: `artifacts/fly/wrela-load-20260224121938-write-load-report.json`
4. Fly cluster drill: `artifacts/fly/wreladb-lab-20260224122453-drill-report.md`
5. Fly chaos loop: `artifacts/fly/wreladb-lab-20260224122821-chaos-20260224122853/summary.md` (`20/20` cycles passed)

Jupiter coordinated cutover (no mixed-version support):

1. Treat upgrades as coordinated cluster cutovers, not rolling mixed-version deploys.
2. Deploy the same runtime build to all nodes in the wave; do not run old/new Jupiter runtime behavior mixed in one quorum.
3. Validate after cutover with:
   1. `scripts/fly/wrela_deploy_smoke.sh`
   2. `scripts/fly/wrela_write_load_test.sh`
   3. `scripts/fly/wreladb_cluster_drill.sh`
   4. `scripts/fly/wreladb_chaos_loop.sh`
4. Rollback path:
   1. Roll back the full cluster to the prior known-good runtime build.
   2. Keep `ReplicatedLogBackend` compatibility paths (`DualWal`, `ShadowCanonical`, `CanonicalOnly`) for validation and controlled rollback testing.
5. Do not claim mixed-version safety for this Jupiter wave.

Private RPC serialization A/B harness:

```bash
WRELADB_PRIVATE_RPC_WIRE_FORMAT=json \
WRELA_PRIVATE_RPC_PERF_DURATION_SECONDS=10 \
WRELA_PRIVATE_RPC_PERF_CONCURRENCY=16 \
WRELA_PRIVATE_RPC_PERF_OPS_PER_BATCH=16 \
WRELA_PRIVATE_RPC_PERF_PAYLOAD_BYTES=128 \
cargo test -p wrela_runtime --test db_private_rpc_perf -- --ignored --nocapture

WRELADB_PRIVATE_RPC_WIRE_FORMAT=binary \
WRELA_PRIVATE_RPC_PERF_DURATION_SECONDS=10 \
WRELA_PRIVATE_RPC_PERF_CONCURRENCY=16 \
WRELA_PRIVATE_RPC_PERF_OPS_PER_BATCH=16 \
WRELA_PRIVATE_RPC_PERF_PAYLOAD_BYTES=128 \
cargo test -p wrela_runtime --test db_private_rpc_perf -- --ignored --nocapture
```

It emits artifacts under `.artifacts/perf/local-db-write/private-rpc/`.

## Fly Deploy Defaults

`wrela deploy` now enforces quorum-safe cluster defaults out of the box:

- `RF=3`
- `WQ=2`
- deterministic shard routing enabled via logical shards + active raft groups
- Fly private-RPC defaults (`mTLS=off` on trusted Fly WireGuard mesh)

You can override cluster shape per deploy:

```bash
wrela deploy . \
  --target=fly \
  --app=my-app \
  --machines=5 \
  --rf=5 \
  --wq=3 \
  --logical-shards=64 \
  --active-groups=8
```

For multi-region + sovereignty + checkpoint locality, add `wrela.deploy.toml` in the app root and run `wrela deploy` normally (or pass `--deploy-policy=...`). The deploy generator emits region-aware checkpoint env maps for runtime-local bucket/endpoint selection.

Example policy:

```toml
[cluster]
target_voters = 5
replication_factor = 5
write_quorum = 3
logical_shards = 64
active_groups = 8

[regions]
ord = 3
iad = 2

[checkpoint]
backend = "s3"
s3_prefix = "tenant-a/checkpoints"
s3_bucket_by_region = { ord = "tenant-a-us", iad = "tenant-a-us-east" }
s3_region_by_region = { ord = "auto", iad = "auto" }
```

## Fly Validation Workflows

Use these scripts for deterministic deploy/runtime validation on temporary Fly apps:

- Health contract:
  - `/api/live` is process liveness only (used by Fly service/machine checks).
  - `/api/health` and `/api/probe/mesh` are strict mesh readiness gates (used by deploy/smoke/load gate logic).

- Deploy smoke with strict gates + rolling redeploy traffic:

```bash
scripts/fly/wrela_deploy_smoke.sh
```

- End-to-end write load run (sustained writes + report artifact):

```bash
scripts/fly/wrela_write_load_test.sh
```

Write-load acceptance profile is tiered and fail-closed:
- Stage A: `60s`, `8` writers, no rolling redeploy, required failure rate `0.0%`.
- Stage B: `180s`, `16` writers, rolling redeploy at `+60s`, allowed failure rate `<=0.1%`.

Common tuning envs for the write load script:

```bash
WRELA_WRITE_LOAD_STAGE_A_DURATION_SECONDS=60 \
WRELA_WRITE_LOAD_STAGE_B_DURATION_SECONDS=180 \
WRELA_WRITE_LOAD_STAGE_B_CONCURRENCY=16 \
scripts/fly/wrela_write_load_test.sh
```

Artifacts are written under `artifacts/fly/`.
