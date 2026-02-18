# Repository Guidelines

The language spec (language/spec/spec.wr) is sacred. Don't you dare touch it.

## Project Structure & Module Organization

Wrela is a Rust workspace with two crates and language assets:

- `compiler/`: main `wrela` crate, CLI entrypoint (`bin/wrela.rs`), and compiler modules (`lexer`, `parser`, `hir`, `mir`, `backend`).
- `runtime/`: `wrela_runtime` crate and runtime support code in `runtime/src/reactor`.
- `compiler/tests/`: integration tests for CLI, codegen, snapshots, and end-to-end flows.
- `language/spec/spec.wr`: authoritative language specification.
- `language/stdlib/`: standard library `.wr` modules grouped by `data/`, `host/`, and `runtime/`.

## Build, Test, and Development Commands

Use Cargo from the repository root:

```bash
cargo build --workspace                  # build compiler + runtime
cargo test --workspace                   # run all Rust tests
cargo test -p wrela --test cli           # run a focused integration test file
cargo run -p wrela -- --help             # inspect CLI commands/options
cargo run -p wrela -- test . --lane=spec          # execute spec-lane tests
cargo fmt --all                          # format Rust code
cargo clippy --workspace --all-targets   # lint with configured thresholds
```

## Coding Style & Naming Conventions

- Rust style is enforced with `rustfmt.toml` (edition 2024, 4-space indentation, max width 100).
- Keep Rust modules/functions in `snake_case`; types/traits in `PascalCase`; constants in `SCREAMING_SNAKE_CASE`.
- Keep `.wr` files and functions descriptive and consistent with existing stdlib/test patterns (for example, `test_basic`, `runtime/scheduler.wr`).
- Prefer small modules with explicit boundaries (`lexer` -> `parser` -> `hir` -> `mir` -> `backend`).

## Testing Guidelines

- Add integration tests under `compiler/tests/*.rs`; prefer behavior-driven names like `cli_json_diagnostics`.
- For language-level coverage, add `.wr` tests and run them through `wrela test`.
- When changing parsing/type/codegen behavior, include at least one regression test.
- Run `cargo test --workspace` before opening a PR.

## Perf on Fly (Agent Policy)

- Use the Fly-machine pooled workflow by default for perf checks.
- Fly perf is currently `amd64` only.
- Runner inventory in `scripts/perf/fly_pool.json` is pre-provisioned and should be treated as stable for normal perf runs (do not modify pool setup in routine agent workflows).
- Claiming is lock-first and host-global to avoid cross-worktree collisions:
  - lock root defaults to `~/.codex/state/wrela-perf-fly-locks`
  - locks are keyed by `<app>-<machine_id>`
- Run PR perf gate with a pushed SHA (required for reproducibility):

```bash
scripts/perf/fly_pr_perf_gate.sh --sha <commit-sha>
```

- Refresh canonical `main` baseline after merges:

```bash
scripts/perf/fly_refresh_main_baseline.sh --sha <main-sha>
```

- Operational rule:
  - Real perf checks must run the whole suite set (`micro meso macro linux`), not partial suites.
  - Real perf checks default to `PERF_RUNS=10`, with `PERF_WARMUP_RUNS=1` (warmup is discarded), and `PERF_CV_MAX_PCT=10`.
  - `PERF_RUNS=1-3` is only for quick smoke/debug loops and must not be used to claim perf wins or update canonical baseline.
  - Every PR perf run must target a commit SHA available on `origin`.
  - Every perf run should start/stop machines through the Fly scripts; avoid ad-hoc manual lifecycle changes.

- Canonical baseline pointer is `.artifacts/perf/main/CANONICAL.json`.
  - Update it only when amd64 run passes and the target SHA is still current `main` head.
  - If run result is `infra_unavailable` or `perf_failed`, do not advance canonical baseline.

- Deprecated workflows:
  - Do not use any `scripts/perf/gcp_*` scripts.
  - Do not use ad-hoc `gcloud` perf flows.
  - Do not run perf from mutable branch heads; always use pushed commit SHA.
  - Do not rebuild/reprovision Fly pool infrastructure during ordinary perf runs.

### Latest Main Benchmark (Where To Look)

- The latest canonical main benchmark is always:
  - `.artifacts/perf/main/CANONICAL.json`
- The canonical run's full artifacts live under:
  - `.artifacts/perf/main/<sha>/`
- Branch-vs-main comparison flow for agents:
  1. Read `.artifacts/perf/main/CANONICAL.json` to get canonical `sha` and `summary_path`.
  2. Run branch perf via `scripts/perf/fly_pr_perf_gate.sh --sha <branch-sha>`.
  3. Compare branch run summary/artifacts under `.artifacts/perf/fly/<run-id>/` against canonical artifacts in `.artifacts/perf/main/<sha>/`.
