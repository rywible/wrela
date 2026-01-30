# Repository Guidelines

## Project Structure & Module Organization
- `crates/compiler`: Wrela compiler library and `wrela` CLI (`bin/wrela.rs`).
- `crates/lsp`: Language server implementation and LSP tests in `crates/lsp/tests`.
- `crates/runtime`: Runtime library, storage components, and runtime tests in `crates/runtime/tests` and `src/storage/tests.rs`.
- `examples/`: Sample `.wr` programs (e.g., `examples/basic.wr`).
- `editors/vscode-wrela`: VS Code extension assets.
- `docs/`: Design and performance notes.
- `scripts/`: Utility scripts (e.g., `scripts/install.sh`).

## Build, Test, and Development Commands
- `cargo build` — Build the full workspace.
- `cargo run -p wrela -- --help` — Run the compiler CLI.
- `cargo run -p wrela-lsp` — Start the language server.
- `cargo test` — Run all workspace tests.
- `cargo test -p wrela_runtime --features test-utils` — Enable runtime storage test utilities.
- `cargo fmt` / `cargo fmt --check` — Format Rust sources.
- `cargo clippy --workspace --all-targets --all-features` — Lint with project clippy rules.

## Coding Style & Naming Conventions
- Rust edition is 2024; formatting is managed by `rustfmt` with 4-space indentation and 100-char line width.
- Keep modules and files focused by subsystem (compiler, runtime, lsp).
- Use clear, descriptive names for Wrela language artifacts (e.g., `lexer`, `parser`, `hir`, `mir`).

## Testing Guidelines
- Tests live in `crates/*/tests` and inline `src/*/tests.rs`.
- Use `cargo test -p <crate>` to scope runs (e.g., `cargo test -p wrela`).
- Runtime storage tests require `--features test-utils`.
