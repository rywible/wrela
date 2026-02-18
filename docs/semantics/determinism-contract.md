# Determinism Contract

This document defines deterministic behavior required by certification.

## Guarantees

1. Certified test runs must produce stable outcome signatures.
2. Differential baseline/alt pipelines must not diverge.
3. Virtual time mode supports deterministic clock/sleep behavior in tests.

## Enforcing Tests And Gates

- Determinism and differential gates: `/Users/ryanwible/projects/wrela/compiler/bin/wrela.rs`
- Virtual time behavior: `/Users/ryanwible/projects/wrela/runtime/src/host.rs`

## ABI Impact

Determinism controls are non-breaking and runtime-internal. ABI version remains authoritative.
