# Runtime Rewrite Phase-0 Baseline Report

Date: 2026-02-08
Purpose: lock reproducible baseline before rewrite deltas.

## Environment

- Host: `Ryans-MacBook-Air.local`
- OS: `Darwin 24.6.0 arm64`
- Rust: `rustc 1.92.0 (ded5c06cf 2025-12-08)`
- Cargo: `cargo 1.92.0 (344c4567c 2025-10-21)`
- Timestamp (UTC): `2026-02-08T05:15:46Z`

## Baseline Commands (Copy/Paste)

```sh
cargo test -p wrela --tests
cargo test -p wrela_runtime --lib
./target/debug/wrela test
cargo test -p wrela --test thin_core_snapshot
cargo test -p wrela --test codegen native_pool_policy_matrix_smoke -- --exact
```

## Functional Baseline (Pass/Fail Matrix)

| Command | Result |
| --- | --- |
| `cargo test -p wrela --tests` | PASS |
| `cargo test -p wrela_runtime --lib` | PASS |
| `./target/debug/wrela test` | PASS (40 passed, 0 failed) |
| `cargo test -p wrela --test thin_core_snapshot` | PASS |
| `cargo test -p wrela --test codegen native_pool_policy_matrix_smoke -- --exact` | PASS |

## Perf Baseline (p50/p95/p99)

Generated via `core/spec/runtime_rewrite/perf_harness.sh baseline`.

| Scenario | p50 (s) | p95 (s) | p99 (s) | Samples |
| --- | --- | --- | --- | --- |
| `pool_queue_mpsc_multi_producer` | 0.07 | 0.07 | 0.07 | 3 |
| `native_pool_backpressure_config_smoke` | 0.30 | 0.30 | 0.30 | 3 |

## Symbol Surface Baseline

- Snapshot file: `core/spec/thin_core_snapshot.txt`
- ABI version: `4`
- Intrinsics count: `31`
- Runtime export count: `88`

## Notes

- This baseline is the reference point for later regression budget decisions.
- Any future symbol change must update the snapshot intentionally and pass hardened tests.
