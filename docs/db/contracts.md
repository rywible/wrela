# Wrela DB Contracts

## Correctness

- `ACK` is emitted only after durable WAL append in the local kernel path.
- Keyspace is namespace-scoped and deterministic.
- `expected_version` mismatches fail deterministically.
- Quorum-rejected batches must not leak into subsequent accepted batches.
- Strong reads compare full HLC ordering (`physical + logical`), not physical time only.

## Limits

- `key <= 1 KiB`
- `value <= 1 MiB`
- `batch <= 4 MiB`
- `batch ops <= 1024`

## Availability

- Single-node kernel path is deterministic and crash-recoverable from WAL replay.
- Replicated append path enforces `prev_log_index/prev_log_term` log-matching and conflict-index
  rejection semantics before accepting append payloads.
