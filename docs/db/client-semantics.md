# DB Client Retry and Idempotency Semantics

This document defines edge behavior for retry and ambiguity handling.

## Required Behaviors

- `NOT_LEADER`: write was sent to follower. Response must include:
  - leader redirect hint
  - retry hint (`retry_after_ms`)
- `RETRY_AFTER`: temporary pressure signal. Client retries with backoff.
- `OCC_MISMATCH`: optimistic write conflict; retry must be caller-driven.
- Idempotency token replay:
  - same token + same payload returns same commit version
  - same token + different payload must fail with argument error

## Timeout Ambiguity Rule

When a client times out after submit (commit unknown), retrying with the same idempotency token must be safe and must not duplicate writes.

## Conformance

```bash
cargo test -p wrela_runtime --test db_client_conformance -- --nocapture
```
