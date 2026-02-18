# Invariant Checker

The invariant checker validates adversarial history traces for:

- no lost acknowledged writes,
- exact (non-substring) typed read observations against acknowledged values,
- no duplicate transaction commits.
- explicit insufficient-observation outcomes when no post-ack read exists.

## Local Execution

```bash
scripts/db-jepsen/check_history.sh
```

The script runs:

- deterministic synthetic history assertions
- adversarial checker bypass regression assertions
- insufficient-observation coverage assertions
- live DB trace assertions generated from real runtime operations

You can run them directly:

```bash
cargo test -p wrela_runtime --test db_invariant_history invariant_checker_accepts_consistent_history
cargo test -p wrela_runtime --test db_invariant_history invariant_checker_rejects_substring_and_lexicographic_cheats
cargo test -p wrela_runtime --test db_invariant_history invariant_checker_flags_insufficient_observation
cargo test -p wrela_runtime --test db_invariant_history invariant_checker_accepts_live_db_trace
```

## Failure Contracts

Typed failures emitted by the checker:

- `LostAcknowledgedWrite`
- `DirtyRead`
- `DuplicateCommit`
- `InsufficientObservation`
