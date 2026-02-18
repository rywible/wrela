# Unsafe Runtime Boundary

Unsafe systems primitives are quarantined to privileged runtime/compiler-backend implementation code.

## Guarantees

1. Unsafe Rust usage is allowed only in allowlisted files.
2. Compiler and user-facing code paths are forbidden from introducing unsafe usage.
3. Runtime-only unsafe helpers are not exposed through user-facing Wrela stdlib APIs.

## Enforcing Tests And Gates

- Allowlist checker: `/Users/ryanwible/projects/wrela/scripts/governance/check_unsafe_allowlist.sh`
- Integration test gate: `/Users/ryanwible/projects/wrela/compiler/tests/unsafe_quarantine.rs`
- Unsafe allowlist definition: `/Users/ryanwible/projects/wrela/runtime/unsafe_allowlist.txt`

## ABI Impact

Unsafe boundary enforcement itself is non-breaking. Any new exported symbol remains subject to thin-core snapshot policy.
