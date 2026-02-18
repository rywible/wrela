# DB Encoding + Zero-Copy Strategy

## Canonical Codec

- Batch command frame: `WR` magic + version + kind + optional expected version + length-prefixed namespace/key/value.
- Value envelope: `V1` magic + length + payload.
- Deterministic wire layout:
  - big-endian fixed-width integers
  - no JSON in hot paths
  - no implicit string transcoding in runtime codec internals

## Hot-Path Adoption

- Batch submit path (`wr_db_submit_batch`):
  - encode canonical put frame into thread-local scratch buffer
  - decode via borrowed byte slices (zero-copy decode view)
  - submit decoded fields to DB core
- Point/range read path (`wr_db_read_point`, `wr_db_read_range`):
  - decode value envelope in legacy-aware mode
  - return payload bytes directly to runtime bytes object creation

## Buffer Reuse

- Thread-local scratch buffer (`runtime/src/db/abi/buffers.rs`) is reused across ABI calls.
- Encode helpers write into caller-provided buffers to avoid per-call transient allocations.

## Measurement Plan

- Reported metrics:
  - `allocs/op`
  - `bytes_copied/op`
  - encode/decode ops/sec
- Codec microbench artifact command:
  - `bash scripts/db-bench/codec.sh`
  - emits `artifacts/codec-bench.json`
- Gate tests:
  - `cargo test -p wrela_runtime db::codec::tests -- --nocapture`
- Existing baseline command:
  - `bash scripts/db-bench/baseline.sh`

## Guardrails

- Any ABI expansion must update `docs/db/abi-boundary.md`, ABI snapshot tests, and governance issue `WRE-509`.
- Codec failures must fail closed with explicit `DbError` variants (no silent fallback except legacy raw-value decode path).
