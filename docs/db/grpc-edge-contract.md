# gRPC Edge Contract (Phase 3 Slice)

## Typed Error Mapping

- `NOT_LEADER`: follower received write; response includes `leader_node_id` and retry hint.
- `RETRY_AFTER`: backpressure/limit path; response includes `retry_after_ms`.
- `OCC_MISMATCH`: optimistic concurrency expectation failed.
- `INVALID_ARGUMENT`: malformed request fields.
- `UNAVAILABLE`: transient storage/runtime unavailability.

## Write Retry Semantics

- `write_batch` accepts optional `idempotency_token`.
- Same token on the leader replays prior successful response without re-applying writes.
- Retry after `NOT_LEADER` with unchanged token must be safe and deterministic.
- Reusing a token with a different write payload is rejected as `INVALID_ARGUMENT` (`IDEMPOTENCY_TOKEN_REUSE_MISMATCH`).

## Request/Response Examples

Write request:

```json
{
  "handle": 7,
  "ops": [
    {
      "put": {
        "namespace": "core",
        "key": "k1",
        "value": "v1"
      }
    }
  ],
  "idempotency_token": "tok-1"
}
```

`NOT_LEADER` response:

```json
{
  "code": "NOT_LEADER",
  "message": "NOT_LEADER: redirect to node-b",
  "leader": { "leader_node_id": "node-b" },
  "retry": { "retry_after_ms": 25 }
}
```

Success response:

```json
{
  "commit_version": 42,
  "idempotent_replay": false
}
```

## Runtime Tests

- `cargo test -p wrela_runtime write_batch_returns_not_leader_with_hint -- --nocapture`
- `cargo test -p wrela_runtime write_batch_idempotency_token_replays_same_commit_without_duplicate_apply -- --nocapture`
- `cargo test -p wrela_runtime write_retry_after_leader_change_succeeds_with_same_token -- --nocapture`
- `cargo test -p wrela_runtime write_batch_reused_token_with_different_payload_is_rejected -- --nocapture`
