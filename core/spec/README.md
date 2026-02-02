# Wrela Language Spec (Baseline)

Date: 2026-02-01

This spec describes **current, implemented behavior** of the Wrela language.
If behavior differs between intent and implementation, the implementation wins.

This baseline is derived from:
- `.plans/language.md`
- `.plans/language_tour.wr`
- Compiler semantics in `core/compiler/**`
- Runtime behavior in `core/runtime/**`

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

See: `.plans/language.md` (Sections 2–3).

## 2) Syntax (Surface Grammar)
This section defines the legal surface syntax:
- Modules and `use`
- Functions (`to`)
- Classes (`A`/`An`), fields (`has`), methods (`can`)
- Expressions and operators
- Control flow (`if`, `match`, `while`, `for`)

See: `.plans/language.md` (Sections 1–8) and parser grammar in:
- `core/compiler/parser/grammar/*.rs`

## 3) Names, Modules, and Entry Point
- Source files use `.wr`.
- `use` is top-level only; import rules are enforced by the compiler.
- Only the entry module defines `to run() -> Type`.

See: `.plans/language.md` (Section 1).

## 4) Types and Static Semantics
- Explicit return types are required.
- Parameters require type annotations.
- Public by default; `private:` blocks apply at top-level and inside classes.

Type rules and result handling are enforced in:
- `core/compiler/hir/typeck.rs`
- `core/compiler/hir/semantic.rs`

## 5) Expressions and Evaluation Order
Defines:
- Operators and precedence
- `match` / `otherwise`
- `err` and Result behavior
- Evaluation order for expressions

See: `.plans/language.md` (Sections 5–9).

## 6) Error Model and Results
- `Result` is a first-class type.
- `err` produces a Result error.
- Must-handle rules for Result are enforced by the compiler.

See: `.plans/language.md` (Result section) and `core/compiler/hir/typeck.rs`.

## 7) Actors and Concurrency
- `detach`, `spawn`, `await`, and `fire` semantics.
- Pool objectives and backpressure behavior.

See: `.plans/language.md` (Actor sections) and:
- `core/compiler/hir/typeck.rs`
- `core/runtime/src/actor.rs`
- `core/runtime/src/scheduler.rs`

## 8) Builtins and Standard Library Surface
The compiler recognizes a fixed set of built-in bindings.
The standard library surface lives in:
- `core/compiler/stdlib/core.wr`

Runtime implementations live in:
- `core/runtime/src/*.rs`

See: `.plans/language.md` (Builtins sections).

## 9) Unspecified/Implementation-Defined Behavior
Explicitly document any behavior that is unspecified or implementation-defined.
Example candidates:
- Map iteration ordering
- Integer overflow
- Error message text

## 10) Spec Tests
Spec tests live under `tests/spec/**` and are required for any spec change.
They are ordinary Wrela tests using `assert value` / `assert identity`.

The authoritative spec file is:
- `core/spec/spec.wr` (symlinked to `tests/spec/spec.wr` for execution)
- `core/spec/stdlib_surface.wr` (symlinked to `tests/spec/stdlib_surface.wr`)

Run:
- `wrela test` (from repo root)
