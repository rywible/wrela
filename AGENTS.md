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

## Perf on GCP (Agent Policy)

- Use the Spot-based dual-arch workflow by default for perf checks. Do not hand-roll `gcloud` commands unless debugging a script.
- Build/refresh prewarmed image families first (or when stale):

```bash
scripts/perf/gcp_build_perf_images.sh
```

- Run PR perf gate (both `amd64` and `arm64`, merge-blocking on regression):

```bash
scripts/perf/gcp_pr_perf_gate.sh --sha <commit-sha>
```

- Refresh canonical `main` baseline after merges:

```bash
scripts/perf/gcp_refresh_main_baseline.sh --sha <main-sha>
```

- Defaults are strict unless explicitly overridden:
  - `GCP_USE_SPOT=1`
  - `GCP_SPOT_MAX_RETRIES=3`
  - `GCP_SPOT_BACKOFF_SEC=20`
  - `GCP_ALLOW_FALLBACK_ONDEMAND=0`
  - `GCP_AMD64_IMAGE_FAMILY=wrela-perf-amd64`
  - `GCP_ARM64_IMAGE_FAMILY=wrela-perf-arm64`
  - `FORCE_REBUILD_WRELA=1`

- Operational rule:
  - Do not run `gcp_build_perf_images.sh` for ordinary PR perf runs.
  - Use prebuilt image families for PR runs, and only run image builds on maintenance cadence (or after toolchain/base-image changes).
  - Every PR perf run must rebuild `wrela` from the current checked-out branch/worktree before running suites.
  - Real perf checks must run the whole suite set (`micro meso macro linux`), not partial suites.
  - Real perf checks should use at least `PERF_RUNS=5` for stable stats; use more only when investigating noisy deltas.
  - `PERF_RUNS=1-3` is only for quick smoke/debug loops and must not be used to claim perf wins or update canonical baseline.

- Canonical baseline pointer is `.artifacts/perf/main/CANONICAL.json`.
  - Update it only when dual-arch run passes and the target SHA is still current `main` head.
  - If run result is `infra_preempted` or `perf_failed`, do not advance canonical baseline.

### Latest Main Benchmark (Where To Look)

- The latest canonical main benchmark is always:
  - `.artifacts/perf/main/CANONICAL.json`
- The canonical run's full artifacts live under:
  - `.artifacts/perf/main/<sha>/`
- Branch-vs-main comparison flow for agents:
  1. Read `.artifacts/perf/main/CANONICAL.json` to get canonical `sha` and `summary_path`.
  2. Run branch perf via `scripts/perf/gcp_pr_perf_gate.sh --sha <branch-sha>`.
  3. Compare branch run summary/artifacts under `.artifacts/perf/gcp/<run-id>/` against canonical artifacts in `.artifacts/perf/main/<sha>/`.
