# Crate Split Decision

Date: 2026-04-17
Phase: 52
Candidate split: extract the `wrela` CLI into a first workspace crate while keeping the binary name
stable.

## Decision

Defer the CLI crate split in Phase 52.

The repo should take the real-module boundary win now and postpone the crate split until the
remaining dependency inversions would let the split buy a real day-to-day rebuild reduction.

## Why The Split Is Deferred

### 1. Non-CLI integration tests still depend on the binary target

The following non-CLI integration surfaces still invoke `CARGO_BIN_EXE_wrela` directly:

- `compiler/tests/preview_project.rs`
- `compiler/tests/contract_blackbox.rs`
- `compiler/tests/spec_project_integrity.rs`
- `compiler/tests/extraction_regression.rs`
- `compiler/tests/repo_smoke.rs`

That means the binary is still part of more than the CLI test surface.
A crate split now would move files around without actually isolating ownership enough.

### 2. The default repo lanes still measure the workspace as a whole

The current named workflows still run through workspace-wide cargo surfaces:

- `just check` -> `cargo check --workspace`
- `just test-all` -> `cargo test --workspace`
- `just test-clean` -> cleanroom `cargo test --workspace`

Until there is a deliberate library-only lane that can skip the CLI package, a new crate boundary
would not change the cost of the canonical repo commands enough to justify the churn.

### 3. Binary ownership is still anchored in the compiler crate

`compiler/Cargo.toml` still owns:

- `[[bin]] name = "wrela"`
- the CLI module tree under `compiler/bin/wrela/`

That is still the right place to keep the code until binary ownership, test ownership, and lane
ownership can move together.

## Evidence That Still Makes Phase 52 Worthwhile

Phase 52 does land the lower-risk boundary win now:

- `compiler/hir/typeck.rs` is now a real module tree under `compiler/hir/typeck/`
- `compiler/query_exec/mir.rs` is now a real module tree under `compiler/query_exec/mir/`
- the CLI command handler tree is now explicit modules under `compiler/bin/wrela/commands/`

That gives contributors smaller ownership islands without forcing a crate split that the current
repo lanes would immediately blur again.

The Phase 52 compile-burst report at `.artifacts/devloop/phase52-baseline.json` backs that up:

- warm `just check`: `1627` ms
- warm frontend edit burst: `1784` ms
- warm query-exec edit burst: `1838` ms
- warm CLI edit burst: `1988` ms
- cleanroom `just check-clean`: `18853` ms
- warm `cargo test --workspace`: `529104` ms
- cleanroom `cargo test --workspace`: `670721` ms

The CLI touch is not materially cheaper than the other representative edit scopes yet.
The meaningful pain today is still workspace-wide test ownership and binary coupling, not CLI-only
compile cost in isolation.

## What Must Change Before The Split Is Worth Reconsidering

1. Move or invert the non-CLI binary consumers so they can prove library behavior without always
   building `CARGO_BIN_EXE_wrela`.
2. Define at least one deliberate library-only `just` lane and include it in the devloop harness.
3. Decide the permanent ownership boundary for the `wrela` binary, the perf engine, and the CLI
   integration tests.
4. Re-run the Phase 52 compile-burst protocol after those inversions land.

Only then should the repo decide whether `wrela_cli/` buys enough local-loop and navigation value
to justify the split.
