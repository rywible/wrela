# Baseline Spec Verification Notes

This file lists areas that require verification against compiler/runtime behavior
while producing the baseline spec. It is not normative.

## High-Priority Checks
- Numeric types: integer size/range, float behavior, overflow handling.
- Equality semantics for lists/maps/objects; pointer vs structural equality.
- String interpolation evaluation order and escaping rules.
- Map iteration ordering and determinism.
- Evaluation order in binary expressions and function arguments.
- Match exhaustiveness and `otherwise` semantics.
- Result must-handle rules and where `err` is permitted.
- Actor scheduling semantics: fairness, mailbox ordering.
- `await`/`fire` restrictions and required actor context.
- Pool objectives (`latency/throughput/conservation/balance`) behavior and defaults.

## Runtime Builtins Consistency
- Builtin list in `.plans/language.md` vs `core/compiler/hir/typeck.rs`.
- Builtin implementations in `core/runtime/src/lib.rs`.
- Stdlib surface in `core/compiler/stdlib/core.wr`.

## Module and Import Rules
- `use` resolution rules and private import enforcement.
- `run` entrypoint restrictions enforced by `core/compiler/hir/project.rs`.

## Error Reporting Stability
- Are error messages stable enough for golden `.err` tests?
- If not, restrict tests to stdout-only or structured error codes.

