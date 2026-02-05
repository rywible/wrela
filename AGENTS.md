# Repository Guidelines

## Project Structure & Module Organization

- `core/`: Wrela compiler, runtime, spec, and tooling. The canonical spec is `core/spec/spec.wr`.
- `apps/hub/`: Hub service (Wrela runtime app).
- `tests/spec/`: Executable spec tests (see `tests/spec/spec.wr`).
- `packages/`: Experimental/incubating/maintained workspaces.
- `rfcs/`: RFC lifecycle storage.
- `.wrelahub/`: Governance metadata, templates, and policies.

## Build, Test, and Development Commands

- `wrela test`: Run executable spec tests under `tests/spec/`.
- `cargo build`: Build the Rust workspace crates under `core/`.
- `cargo fmt`: Format Rust code using the repo’s `rustfmt.toml`.
- `cargo clippy`: Lint Rust code using the repo’s `clippy.toml`.

## Coding Style & Naming Conventions

- Rust code should be formatted with `cargo fmt` and kept warning-free under `cargo clippy`.
- Wrela spec changes should keep sections in `core/spec/spec.wr` stable and update matching tests in `tests/spec/`.
- Use clear, descriptive file and directory names; prefer existing patterns in `core/`, `apps/`, and `tests/`.

## Testing Guidelines

- Spec changes must include tests under `tests/spec/` that fail if the relevant spec section is removed.
- Run `wrela test` before submitting changes that affect the spec or runtime behavior.

## Commit & Pull Request Guidelines

- All commits must include a DCO sign-off. Example:
  `Signed-off-by: Your Name <you@example.com>`
- One open PR per contributor (early phase).
- PRs must follow the structured fields required by the repository and are subject to domain-scoped review.
- For design changes, open an RFC draft in `rfcs/`. For behavioral gaps, submit an Analysis Report using `.wrelahub/templates`.

## Governance & Security

- This repo is governance-driven and spec-first. Review `CHARTER.md` and `core/spec/spec.wr` before major changes.

## Wrela Language Guide

- Always reference the latest spec for what is available in the language
- Always reference the latest stdlib at `core/compiler/stdlib/*` for available libs
- Wrela code is faux english readable. declarations of function with `to` should be followed by verb form function names for grammatical correctness.
- Checks use `check`/`checks` with explicit `-> Boolean` return types and are evaluated via `given` (no normal call syntax).
- Proper use of A or An should be used before class declarations depending on class identifier.
- No abbreviations of identifiers. Prefer specific identifier names over vague ones.
- Wrap long lines after a certain point. Wrapping inside of parentheses for passing function or class args is valid.
- Prefer readable code over clever code. Comments should clarify ambiguous intent, but code should be mostly self-documenting
- Avoid ambiguous parameter and field names. Prefer explicit, stateful names (e.g., `*_state`, `*_reason`) over vague labels like `current`, `target`, or `reason`.
- Avoid redundant “builder” helpers that wrap a single error constructor. Instantiate the error directly where the transition is validated.
- Use correct English articles in Wrela identifiers and docs. Choose `A` vs `An` based on pronunciation, not the letter.
- Prefer explicit imports over wildcard imports. Import only the names you use.
- Tests should be run with sane timeouts. If a test is hanging, it is most likely not slow, there is most likely an infinite loop in the code that you've produced. Err on finding bugs rather than just thinking tests are slow.
