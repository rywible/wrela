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
