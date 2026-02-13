# Memory And Values

This document captures memory/value guarantees used by the current runtime and compiler.

## Guarantees

1. Runtime reference-counted heap values are retained/released through runtime primitives.
2. Arena-owned pointers are not refcounted.
3. Value identity equality and deep equality are distinct operations.

## Enforcing Tests And Gates

- Runtime RC behavior: `/Users/ryanwible/projects/wrela/runtime/src/lib.rs`
- MIR no-RC fast paths: `/Users/ryanwible/projects/wrela/compiler/tests/codegen.rs`
- Deterministic certification pipeline: `/Users/ryanwible/projects/wrela/compiler/bin/wrela.rs`

## ABI Impact

These guarantees rely on stable value representation and runtime exports listed in
`/Users/ryanwible/projects/wrela/language/spec/thin_core_snapshot.txt`.
