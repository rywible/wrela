# Failure And Recovery

This document captures failure boundary expectations between recoverable and fatal behavior.

## Guarantees

1. Recoverable host/runtime failures are represented as `Result` values where applicable.
2. Fatal failures remain explicit crash boundaries.
3. Certification and test harnesses surface deterministic failure outcomes.

## Enforcing Tests And Gates

- Type and result handling checks: `/Users/ryanwible/projects/wrela/compiler/hir/typeck.rs`
- Runtime host and result paths: `/Users/ryanwible/projects/wrela/runtime/src/host.rs`
- Determinism and differential gates: `/Users/ryanwible/projects/wrela/compiler/bin/wrela.rs`

## ABI Impact

Failure semantics rely on stable `Result` runtime exports in
`/Users/ryanwible/projects/wrela/language/spec/thin_core_snapshot.txt`.
