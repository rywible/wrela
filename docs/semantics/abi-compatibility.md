# ABI Compatibility

This document captures ABI compatibility commitments for runtime/compiler coordination.

## Guarantees

1. Runtime ABI version must match compiler backend expectations.
2. Runtime export and intrinsic symbol classes are snapshot-gated.
3. New capability markers must be additive and non-breaking.

## Enforcing Tests And Gates

- ABI/version checks: `/Users/ryanwible/projects/wrela/compiler/tests/thin_core_snapshot.rs`
- Snapshot source of truth: `/Users/ryanwible/projects/wrela/language/spec/thin_core_snapshot.txt`

## ABI Impact

Any ABI surface change requires intentional snapshot update and passing thin-core tests.
