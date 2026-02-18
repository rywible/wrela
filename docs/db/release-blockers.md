# DB GA Release Blockers

All blockers are hard fail. No soft passes.

## Correctness Blockers

- Full runtime deterministic suite must pass.
- CDC monotonicity and checkpoint behavior must pass.
- Analytics federated planner/merge determinism must pass.

Command:

```bash
cargo test -p wrela_runtime
```

## Compatibility Blockers

- Versioned format boundary checks must pass for WAL/snapshot/RPC.

Command:

```bash
cargo test -p wrela_runtime --test db_upgrade_compat
```

## Analytics GA Blockers

- Analytics public API tests must pass.
- Analytics durability/restore + GA gate tests must pass.

Commands:

```bash
cargo test -p wrela_runtime --test db_analytics_public_api
cargo test -p wrela_runtime --test db_analytics_durability_gate
```

## Operational Blockers

- No blocker command may be skipped.
- Output artifacts (test logs) must be attached to release evidence.
