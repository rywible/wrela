# Runtime Test Inventory

## Test Tiers

| Tier | Name | What | How to run | Infrastructure |
|------|------|------|-----------|----------------|
| 0 | fast | All non-`#[ignore]` integration + unit tests | `just test` | None (local) |
| 1 | perf | In-process 3-node cluster perf harnesses (`#[ignore]`) | `just test-perf` | None (local) |
| 2 | cluster | Multi-process local cluster ops tests (`#[ignore]`) | `just test-cluster` | Requires wreladb-lab binary |
| 3 | fly | Fly.io deployment, load, and chaos tests (shell scripts) | `just fly-smoke`, `just fly-chaos`, etc. | Requires `FLY_API_TOKEN` + Fly account |

### Tier 0: fast

Run all non-ignored runtime tests:

```
just test
```

Subsets:

```
just test-consensus   # consensus fault / raft / chaos / failover / quorum
just test-wal         # WAL failure isolation
just test-compiler    # compiler CLI tests
```

### Tier 1: perf

Manual benchmarks that spin up an in-process 3-node cluster. Always run in release mode.

```
just test-perf        # local write throughput harness
just test-perf-rpc    # private RPC throughput harness
```

Produces JSON artifacts under `.artifacts/perf/`.

### Tier 2: cluster

Multi-process local cluster tests. Require the `wreladb-lab` binary to be built and on `$PATH`.

```
just test-cluster         # all 5 cluster test binaries
just test-cluster-smoke   # just the smoke test
```

### Tier 3: fly

Shell scripts that deploy to Fly.io and run remote tests.

```
just fly-smoke              # deploy + health check
just fly-load               # write load test
just fly-chaos              # chaos loop
just fly-drill              # cluster drill
just fly-perf <sha>         # PR perf gate
just fly-baseline <sha>     # refresh main baseline
```

---

## Perf Harness Environment Variables

### `db_write_local_perf` (`just test-perf`)

| Variable | Default | Description |
|----------|---------|-------------|
| `WRELA_LOCAL_PERF_DURATION_SECONDS` | `20` | How long the harness runs |
| `WRELA_LOCAL_PERF_CONCURRENCY` | `32` | Number of concurrent writer tasks |
| `WRELA_LOCAL_PERF_PAYLOAD_BYTES` | `128` | Size of each write payload |
| `WRELA_LOCAL_PERF_WRITER_LANE_COUNT` | `1` | Writer lanes in the cluster config |
| `WRELA_LOCAL_PERF_APPLY_LANE_COUNT` | `max(writer_lane_count, 1)` | Apply lanes |
| `WRELA_LOCAL_PERF_RPC_CHANNELS_PER_TARGET` | `1` | Private RPC channels per replication target |
| `WRELA_LOCAL_PERF_LOGICAL_SHARDS` | `1` | Number of logical shards |
| `WRELA_LOCAL_PERF_ACTIVE_GROUPS` | `1` | Number of active groups |
| `WRELA_LOCAL_PERF_BATCH_WINDOW_US` | `500` | Replication batch window in microseconds |
| `WRELA_LOCAL_PERF_BATCH_MAX_OPS` | `64` | Max operations per replication batch |
| `WRELA_LOCAL_PERF_BATCH_MAX_BYTES` | `262144` | Max bytes per replication batch |
| `WRELA_LOCAL_PERF_MAX_IN_FLIGHT` | `32` | Max in-flight replication batches |
| `WRELA_LOCAL_PERF_MAX_TARGETS` | `256` | Max replication targets |
| `WRELA_LOCAL_PERF_HEDGE_EXTRA` | `1` | Extra hedge replication targets |
| `WRELA_LOCAL_PERF_REPLICATION_FACTOR` | `3` | Replication factor |
| `WRELA_LOCAL_PERF_WRITE_QUORUM` | `2` | Write quorum size |
| `WRELA_LOCAL_PERF_COMMIT_VISIBILITY_MODE` | `async_apply` | Commit visibility mode |
| `WRELA_LOCAL_PERF_LOG_BACKEND` | `dual_wal` | Replicated log backend |
| `WRELA_LOCAL_PERF_REQUIRE_LANE_SPREAD` | `false` | Anti-cheat: require ops spread across lanes |

### `db_private_rpc_perf` (`just test-perf-rpc`)

| Variable | Default | Description |
|----------|---------|-------------|
| `WRELA_PRIVATE_RPC_PERF_DURATION_SECONDS` | `8` | How long the harness runs |
| `WRELA_PRIVATE_RPC_PERF_CONCURRENCY` | `16` | Number of concurrent tasks |
| `WRELA_PRIVATE_RPC_PERF_OPS_PER_BATCH` | `1` | Operations per batch |
| `WRELA_PRIVATE_RPC_PERF_PAYLOAD_BYTES` | `32` | Size of each payload |
| `WRELA_PRIVATE_RPC_PERF_WIRE_FORMAT` | `json` | Wire format for RPC messages |
