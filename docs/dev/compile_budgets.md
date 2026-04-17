# Compile Budgets

Phase 52 turns compile-loop measurements into an explicit repo artifact instead of a vague feeling.

## Current Budget Contract

- `fast_verify` is the only hard budgeted lane today.
  Budget: `60000` ms.
  Surface: `just test`.
- `check_warm`, `check_cleanroom`, `test_workspace_warm`, and `test_cleanroom` are part of the
  default checked-in baseline.
- `frontend_edit_check`, `query_exec_edit_check`, and `cli_edit_check` are compile-burst probes.
  They exist to make edit-scope regressions visible before Phase 53 tightens ownership further.

## Why Only One Hard Budget Exists Today

The repo has one named developer-facing fast lane already: `just test`.
That lane is stable enough to carry a hard timing budget now.

The cleanroom and per-context compile-burst measurements are new in Phase 52.
They are recorded first so the repo has truthful baselines before we decide which of them deserve
future enforcement.

## Where To Read The Numbers

The latest checked baseline lives at:

- `.artifacts/devloop/phase52-baseline.json`

That report includes:

- `scorecard`
- `warm_vs_cleanroom`
- `compile_bursts`
- `cli_boundary_assessment`

## Current Phase 52 Snapshot

From `.artifacts/devloop/phase52-baseline.json` on 2026-04-17:

- `check_warm`: `1627` ms
- `check_cleanroom`: `18853` ms
- cleanroom delta versus warm check: `17226` ms (`11.588x`)
- `test_workspace_warm`: `529104` ms
- `test_cleanroom`: `670721` ms
- cleanroom delta versus warm workspace test: `141617` ms (`1.268x`)
- `frontend_edit_check`: `1784` ms (`+157` ms vs warm check)
- `query_exec_edit_check`: `1838` ms (`+211` ms vs warm check)
- `cli_edit_check`: `1988` ms (`+361` ms vs warm check)
- `fast_verify`: `1988` ms against the `60000` ms hard budget

That snapshot is the evidence behind the current decision to keep the CLI split deferred:
the edit-scope burst is not the bottleneck yet, while the real day-to-day pain sits in the
workspace-wide Rust test surface. The cleanroom deltas still show why the non-incremental path
must stay explicit and rare.

## How To Reproduce

Run the full baseline:

```bash
just baseline-devloop
```

Run only the cleanroom comparison:

```bash
python3 scripts/devloop_measure.py --scenario check_warm --scenario check_cleanroom --scenario test_workspace_warm --scenario test_cleanroom --report-name phase52-cleanroom-compare
```

Run only the compile-burst probes:

```bash
python3 scripts/devloop_measure.py --scenario frontend_edit_check --scenario query_exec_edit_check --scenario cli_edit_check --report-name phase52-edit-bursts
```

## Interpretation Rules

- A warm result is the everyday loop.
- A cleanroom result is the truth-first escape hatch.
- A compile burst is not a full rebuild claim; it is the incremental warm check cost immediately
  after a representative touch.
- The CLI split decision must use both compile data and dependency ownership evidence.
  A fast-looking CLI edit alone is not enough if the workspace still has broad binary coupling.
