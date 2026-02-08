# Next Wave Checkpoint Baseline

Date: 2026-02-08

Purpose: rollback anchor before runtime policy migration.

## Baseline commands

- `cargo test -p wrela --tests` -> pass
- `cargo test -p wrela_runtime --lib` -> pass
- `./target/debug/wrela test` -> pass

## Notes

- Tree was clean before this checkpoint.
- This checkpoint commit is intentionally minimal.
