# Developer-Loop Playbook

Phase 52 measures throughput the way the repo measures everything else: with explicit cache state,
explicit cleanroom escape hatches, and stable scenario ids that map to real edit contexts.

## What Phase 52 Proves

The harness under `scripts/devloop_measure.py` now answers four questions:

- what the default warm incremental loop costs
- what the rare cleanroom truth-first path costs
- how large the compile burst is after representative edit scopes
- whether the first CLI crate split is justified yet

Reports are written under `.artifacts/devloop/`.

The canonical entry points are:

- `just baseline-devloop`
- `python3 scripts/devloop_measure.py --report-name phase52-baseline`

The default baseline covers the core compile-loop scenarios:
warm check, cleanroom check, warm workspace test, cleanroom workspace test, the three edit-scope
bursts, and the fast repo lane.

## Cache-State Contract

Do not hide cache state.

- `warm` means the same resolved command completed once on the same git SHA in the same worktree
  before the measured run.
- edit-scope scenarios warm first, then `touch` the representative file, then run the measured
  command immediately afterward.
- `cleanroom` means `CARGO_INCREMENTAL=0` plus an isolated `CARGO_TARGET_DIR` under
  `.artifacts/cargo-cleanroom/`.

The default developer loop is warm incremental.
The cleanroom path exists for truth-first verification, not as the day-to-day default.

## Scenario Protocol

Each scenario records both the canonical repo command and the exact resolved command it executed.
That keeps the report aligned with the `just` surface while still making the underlying substrate
explicit.

### Warm Incremental Scenarios

- `check_warm`
  Canonical command: `just check`
  Resolved command: `cargo check --workspace`

- `test_workspace_warm`
  Canonical command: `cargo test --workspace`
  Resolved command: `cargo test --workspace`

- `fast_verify`
  Canonical command: `just test`
  Resolved command: `cargo test -p wrela --test repo_smoke` plus
  `cargo run -p wrela -- test language/spec --lane=fast`
  Hard budget: `60000` ms

- `full_verify`
  Canonical command: `just test-all`
  Resolved command: `cargo test --workspace` plus
  `cargo run -p wrela -- test language/spec --lane=full`

### Cleanroom Scenarios

- `check_cleanroom`
  Canonical command: `just check-clean`
  Resolved command:
  `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=.artifacts/cargo-cleanroom/check cargo check --workspace`

- `test_cleanroom`
  Canonical command: `just test-clean`
  Resolved command:
  `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=.artifacts/cargo-cleanroom/test cargo test --workspace`

### Compile-Burst Scenarios

These are touch-only representative edits. They do not mutate source contents.

- `frontend_edit_check`
  Touch `compiler/parser/mod.rs`, then run `just check`

- `query_exec_edit_check`
  Touch `compiler/query_exec/context.rs`, then run `just check`

- `cli_edit_check`
  Touch `compiler/bin/wrela/cli_args.rs`, then run `just check`

## Report Shape

Each Phase 52 report records:

- scenario ids, canonical commands, and resolved commands
- elapsed wall-clock timings
- success/failure and exit codes
- machine tag, git SHA, and git-dirty state
- warmup metadata and touched files
- fast-lane scorecard status (`within_budget`, `missed_budget`, `failed`, or `measured`)
- `warm_vs_cleanroom` comparisons for workspace check and workspace test
- `compile_bursts` for frontend, query-exec, and CLI edit scopes
- `cli_boundary_assessment`, including the split decision and blockers if the split is deferred

The stable baseline report is written to:

- `.artifacts/devloop/phase52-baseline.json`
- `.artifacts/devloop/phase52-baseline-<timestamp>.json`

## Reproducing The Phase 52 Evidence

Use the default baseline when you want the full checked-in picture:

```bash
just baseline-devloop
```

Use targeted reruns when you only need one slice:

```bash
python3 scripts/devloop_measure.py --scenario check_warm --scenario check_cleanroom --report-name phase52-check-compare
python3 scripts/devloop_measure.py --scenario test_workspace_warm --scenario test_cleanroom --report-name phase52-test-compare
python3 scripts/devloop_measure.py --scenario frontend_edit_check --scenario query_exec_edit_check --scenario cli_edit_check --report-name phase52-edit-bursts
```

## Interpreting The CLI Decision

The Phase 52 report does not assume the CLI split is good just because it is a plausible boundary.

- If the CLI boundary materially reduces unrelated rebuilds, the report should say so.
- If it does not yet reduce them, the report should carry the explicit blockers and point to
  `../architecture/crate_split_decision.md`.

That is why the module split work and the crate-split decision are separate deliverables in this
phase.
