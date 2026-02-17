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
cargo run -p wrela -- test language/spec/spec.wr  # execute spec tests
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

## Wrela Authoring Notes (Hard-Earned)

Use this section when writing new `.wr` code in this repo. These are practical constraints from current compiler behavior, not theory.

### 1. Naming rules are strict and enforced at compile time

- Function names must be ASCII `snake_case`.
- Top-level function names are expected to be verb-led (`compile_*`, `try_to_*`, `create_*`, etc.).
- Result-returning functions are expected to start with `try_to_`.
- Functions that act as class factories are expected to use `create_` (or `try_to_create_` for fallible ones).
- Boolean locals/params/check-like names should use `is_` or `has_` prefixes.
- `check` names have shape requirements:
  - top-level checks should contain `_is_` or `_has_`
  - class/interface checks should start with `is_` or `has_`
- Field names are expected to be noun-like (verb-led field names are rejected).
- Collection names are expected to be plural.

Source of truth: `compiler/hir/naming.rs`.

### 2. Prefer explicit, conservative typing patterns

- `Any` is reserved for stdlib and should not be used casually in user code.
- Typed list literals in constructors can trigger type friction; if needed, start with plain `List` and tighten types later.
- When matching `Result`, handle `Ok`/`Err`/`otherwise` explicitly in scaffolding code.

### 3. `check` vs function call syntax matters

- Checks should be called with `given` syntax where required by the typechecker.
- Boolean-returning normal functions are forbidden; predicates must be declared as `check ... -> Boolean`.
- Avoid adding top-level `check` functions that can be auto-generated/fuzzed into failing assertions during certification unless behavior is intentionally robust across broad generated inputs.

### 4. Build mode vs check mode

- `wrela check <path>` can work in single-file mode.
- `wrela build <path>` expects project-mode layout (`src/**`) and runs additional checks/cert flows.
- A scaffold can pass `check` but still fail `build` due to certification lane behavior.

### 5. Keep imports minimal in early scaffolding

- Import only what you use; unused imports can fail or pollute diagnostics.
- Be careful with broad stdlib imports while scaffolding; keep dependency surface small to reduce unrelated naming/type diagnostics.

### 6. Entry-point and top-level constraints still apply

- Entrypoint module should define `to run() -> Type`.
- Avoid top-level executable statements outside allowed declarations.
- Only entry module may define `run`.

## Self-Host Plan Source of Truth

- Canonical project plan: `/Users/ryanwible/projects/wrela/wrela-on-wrela/docs/master-plan.md`
- Keep this file current as implementation progresses; treat it as the master checklist and scope contract for wrela-on-wrela completion.
