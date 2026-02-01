# Wrela Language Spec (Baseline)

Date: 2026-02-01

This spec describes **current, implemented behavior** of the Wrela language.
If behavior differs between intent and implementation, the implementation wins.

This baseline is derived from:
- `docs/language.md`
- `docs/language_tour.wr`
- Compiler semantics in `crates/compiler/**`
- Runtime behavior in `crates/runtime/**`

## 0) Scope and Versioning
- The spec describes the language as shipped by this repo at a given commit.
- Spec changes require corresponding spec tests.
- Any change to `core/spec/**` requires tests under `tests/spec/**`.

## 1) Lexical Structure
- Indentation + colons define blocks.
- Spaces only; tabs are errors.
- Comments use `so:`.
- Identifiers allow letters, digits, `_`, and non-ASCII letters.
- Keywords are reserved.

See: `docs/language.md` (Sections 2–3).

## 2) Syntax (Surface Grammar)
This section defines the legal surface syntax:
- Modules and `use`
- Functions (`to`)
- Classes (`A`/`An`), fields (`has`), methods (`can`)
- Expressions and operators
- Control flow (`if`, `match`, `while`, `for`)

See: `docs/language.md` (Sections 1–8) and parser grammar in:
- `crates/compiler/parser/grammar/*.rs`

## 3) Names, Modules, and Entry Point
- Source files use `.wr`.
- `use` is top-level only; import rules are enforced by the compiler.
- Only the entry module defines `to run() -> Type`.

See: `docs/language.md` (Section 1).

## 4) Types and Static Semantics
- Explicit return types are required.
- Parameters require type annotations.
- Public by default; `private:` blocks apply at top-level and inside classes.

Type rules and result handling are enforced in:
- `crates/compiler/hir/typeck.rs`
- `crates/compiler/hir/semantic.rs`

## 5) Expressions and Evaluation Order
Defines:
- Operators and precedence
- `match` / `otherwise`
- `err` and Result behavior
- Evaluation order for expressions

See: `docs/language.md` (Sections 5–9).

## 6) Error Model and Results
- `Result` is a first-class type.
- `err` produces a Result error.
- Must-handle rules for Result are enforced by the compiler.

See: `docs/language.md` (Result section) and `crates/compiler/hir/typeck.rs`.

## 7) Actors and Concurrency
- `detach`, `spawn`, `await`, and `fire` semantics.
- Pool objectives and backpressure behavior.

See: `docs/language.md` (Actor sections) and:
- `crates/compiler/hir/typeck.rs`
- `crates/runtime/src/actor.rs`
- `crates/runtime/src/scheduler.rs`

## 8) Builtins and Standard Library Surface
The compiler recognizes a fixed set of built-in bindings.
The standard library surface lives in:
- `crates/compiler/stdlib/core.wr`

Runtime implementations live in:
- `crates/runtime/src/*.rs`

See: `docs/language.md` (Builtins sections).

## 9) Unspecified/Implementation-Defined Behavior
Explicitly document any behavior that is unspecified or implementation-defined.
Example candidates:
- Map iteration ordering
- Integer overflow
- Error message text

## 10) Spec Tests
Spec tests live under `tests/spec/**` and are required for any spec change.
They are ordinary Wrela tests using `assert value` / `assert identity`.

Run:
- `wrela test` (from repo root)
