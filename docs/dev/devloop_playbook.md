# Developer-Loop Playbook

Phase 49 measures developer throughput the same way later phases measure rendering and perf closure:
truthfully, with explicit cache state, explicit exclusions, and stable scenario ids.

## What This Measures

The harness under `scripts/devloop_measure.py` measures repo workflow lanes and representative
edit scopes. It writes machine-readable reports under `.artifacts/devloop/`.

The canonical entry points are:

- `just baseline-devloop`
- `python3 scripts/devloop_measure.py --report-name phase49-baseline`

## Warm Versus Cold

Do not hide cache state.

- `warm` means the same resolved command completed once on the same git SHA in the same worktree
  before the measured run.
- For edit-scope scenarios, warm means the priming run happens before the representative file is
  touched, and the measured run happens immediately after that touch.
- `cold` means no priming run was performed for the measured command in the current worktree.

The default Phase 49 baseline uses warm measurements.

## Scenario Protocol

The harness records both the canonical repo command and the raw command it actually executed.
That lets the report stay aligned with the `just` surface while still being runnable on a machine
that only has the repo substrate installed.

### Core Scenarios

- `check_warm`
  Canonical command: `just check`
  Resolved command: `cargo check --workspace`
  Excludes test execution, linking, and authored-world proof surfaces.

- `build_no_run_warm`
  Canonical command: `cargo test --workspace --no-run`
  Resolved command: `cargo test --workspace --no-run`
  Excludes test execution.

- `rust_fast_lane`
  Canonical command: `just test`
  Resolved command: the fast Rust integrity lane plus `wrela test language/spec --lane=spec`
  Excludes the full Rust workspace sweep and closure perf lane.

- `rust_full_lane`
  Canonical command: `just test-all`
  Resolved command: the full Rust workspace sweep plus `wrela test language/spec`
  Excludes lint, fmt-check, and closure perf.

- `perf_smoke`
  Canonical command: `just perf-smoke`
  Resolved command: `cargo run -p wrela -- perf benchmarks/micro --profile=smoke --runs=1`
  Excludes the representative 1080p120 closure protocol.

- `ship`
  Canonical command: `just ship`
  Resolved command: `fmt-check`, `lint`, `test`, `test-all`, then `perf-smoke`
  Excludes `perf-closure`.

### Representative Edit-Context Scenarios

These scenarios use `touch`-style representative edits so later phases can compare blast radius
without mutating source content.

- `frontend_edit_check`
  Touch `compiler/parser/mod.rs`, then run `just check`

- `query_exec_edit_check`
  Touch `compiler/query_exec/context.rs`, then run `just check`

- `cli_edit_check`
  Touch `compiler/bin/wrela/cli_args.rs`, then run `just check`

- `full_workspace_no_run`
  Run `cargo test --workspace --no-run`

- `fast_verify`
  Run `just test`

- `full_verify`
  Run `just test-all`

## Report Shape

Each report records:

- scenario id
- canonical command text
- resolved command text
- elapsed wall-clock duration
- success or failure
- exit code
- machine tag
- git SHA
- cache state (`warm` or `cold`)
- scenario exclusions and notes
- representative touched files when applicable

The Phase 49 baseline report is written to:

- `.artifacts/devloop/phase49-baseline.json`
- `.artifacts/devloop/phase49-baseline-<timestamp>.json`

Failures are part of the truth.
If a lane is red during the Phase 49 baseline run, the report should record that failure rather
than silently narrowing the lane.
