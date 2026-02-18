# Invariant Checker

The invariant checker validates adversarial history traces for:

- no lost acknowledged writes,
- monotonic read behavior per key,
- no duplicate transaction commits.

## Local Execution

```bash
scripts/db-jepsen/check_history.sh
```

The script runs two lanes:

- deterministic synthetic history assertions
- live DB trace assertions generated from real runtime operations

You can run them directly:

```bash
cargo test -p wrela_runtime --test db_invariant_history invariant_checker_accepts_consistent_history
cargo test -p wrela_runtime --test db_invariant_history invariant_checker_accepts_live_db_trace
```

## Failure Contracts

Typed failures emitted by the checker:

- `LostAcknowledgedWrite`
- `DirtyRead`
- `DuplicateCommit`
