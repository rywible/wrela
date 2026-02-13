# Wrela DB Contracts

## Correctness

- `ACK` is emitted only after durable WAL append in the local kernel path.
- Keyspace is namespace-scoped and deterministic.
- `expected_version` mismatches fail deterministically.

## Limits

- `key <= 1 KiB`
- `value <= 1 MiB`
- `batch <= 4 MiB`
- `batch ops <= 1024`

## Availability

- Single-node kernel path is deterministic and crash-recoverable from WAL replay.
- Future replicated paths must preserve no acknowledged-write-loss semantics.

