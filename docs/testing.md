# Wrela Testing Framework (First-Class)

Date: 2026-02-01

This document defines the built-in test runner and assertion API.
Tests are regular Wrela files. There is no external harness or framework required.

## Test Discovery
- Any top-level function named `test_*` is a test.
- Tests live under `tests/**` at the repo root.
- Tests may import normal project modules via `use`.

## Assertions (Keywords)
- `assert value <bool expr>`
- `assert identity <bool expr>`
- `assert_err(result)` (builtin; aborts if `result` is Ok)

Notes:
- Assertions abort the test process on failure.

## Runner Semantics
- Default execution is serial.
- Parallel execution is allowed via a CLI flag.
- Default per-test timeout: 5 seconds.
- Tests run inside the Wrela scheduler runtime (actor-aware).
- No fixtures, setup, or teardown in v1.

## CLI
- `wrela test` runs all tests.
- `wrela test <path>` runs a subset.
- `--jobs=N` controls parallelism (default: 1).
- `--test-timeout-ms=N` sets the per-test timeout (default: 5000).
- Non-zero exit on failure.

## Conventions
- Prefer small, single-behavior tests.
- Avoid shared mutable state across tests.
- Do not rely on ordering unless required.

## Equality Semantics
- `assert value` allows `==`, `!=`, `<`, `<=`, `>`, `>=`. `==` and `!=` use deep
  equality for lists/maps/results and fall back to identity for other object types.
- `assert identity` only allows `==` and `!=` and rejects primitive operands.
