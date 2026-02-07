# Core Thin Reset

Date: 2026-02-07

## Decision

Wrela core is reset to a thin seed:
- Rust compiler remains the trusted seed.
- Rust runtime keeps value model, reference counting, ABI, and actor/pool scheduler.
- Core stdlib keeps foundational modules only.
- High-level product modules are removed from core stdlib/runtime.

## Core stdlib kept

- `actor`
- `bytes`
- `env`
- `fs`
- `io`
- `list`
- `log`
- `map`
- `metrics`
- `parse`
- `pool`
- `runtime`
- `time`

## Core stdlib removed

- `admin`
- `auth`
- `files`
- `http`
- `jobs`
- `pubsub`
- `rate_limit`
- `rbac`
- `realtime`
- `schedule`
- `search`
- `storage`

## Runtime ABI impact

- Runtime ABI version is bumped from `1` to `2`.
- Removed runtime exports align with removed stdlib modules.

## Rationale

- Keep bootstrap path small and reliable.
- Prevent product-surface churn from blocking compiler/runtime iteration.
- Preserve an escape hatch: language implementation can always be rebuilt/fixed from Rust seed.

## Follow-up

- Reintroduce high-level capabilities as ecosystem packages, not as core stdlib/runtime.
