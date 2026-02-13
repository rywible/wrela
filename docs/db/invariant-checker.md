# Invariant Checker

The invariant checker validates adversarial history traces for:

- no lost acknowledged writes,
- monotonic read behavior per key,
- no duplicate transaction commits.

## Local Execution

```bash
scripts/db-jepsen/check_history.sh
```

Or directly:

```bash
cargo test -p wrela_runtime --test db_invariant_history
```

## Failure Contracts

Typed failures emitted by the checker:

- `LostAcknowledgedWrite`
- `DirtyRead`
- `DuplicateCommit`
