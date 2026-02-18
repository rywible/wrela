# DB Hardening Troubleshooting

## Quorum Reject (`durability quorum not reached`)

- Meaning: append responses did not satisfy required term/index durability quorum.
- During joint membership: both outgoing and incoming voter majorities are required.
- First checks:
  - validate active membership state (`joint` vs steady state),
  - verify follower append responses include required `term` + `match_index`,
  - verify follower is not returning conflict-only replies.

## Replication Convergence Reject (`replication convergence ... RETRY_AFTER_MS=25`)

- Meaning: follower catch-up failed to make monotonic progress toward leader log.
- First checks:
  - inspect follower conflict indexes (must strictly reduce `next_index`),
  - validate leader log contiguity and follower term alignment,
  - retry after transport/storage transient is resolved.

## Strong Read Reject (`STRONG_READ_SAFE_TIME_LAG`, `STRONG_READ_UNCERTAINTY_WINDOW`)

- Meaning: requested read timestamp is not yet safe for strong visibility.
- First checks:
  - inspect safe-time diagnostics (`global_safe_time`, region/shard lag),
  - retry using provided `RETRY_AFTER_MS`,
  - use explicit eventual reads only where semantics allow.

## CDC Ack Persist Failure

- Meaning: checkpoint persistence failed; ack was rejected.
- Contract: checkpoint update is copy-on-write; in-memory checkpoint does not advance on failure.
- Durability path: temp file write + `sync_data` + rename + parent-directory `fsync`.
- First checks:
  - storage path health/permissions for checkpoint file,
  - retry ack after storage issue is resolved.

## Membership Mutation Deny (`UNAUTHORIZED_MEMBERSHIP_MUTATION` / `unauthorized rpc`)

- Meaning: unauthenticated compatibility wrapper or non-admin identity attempted cluster membership
  mutation.
- First checks:
  - use `*_authorized` membership APIs (or cert-auth variants),
  - verify identity role maps to `ClusterAdmin`,
  - verify PKI binding/revocation state for cert-auth calls.

## WAL Batch Failure

- Meaning: batch WAL append/sync failed.
- Contract: submit is caller-atomic; no partial in-memory apply is visible on returned error.
- First checks:
  - disk health and fsync latency/failure signals,
  - replay logs for repeated write/sync faults.

## Close Returned `false`

- Meaning: DB handle was closed/unregistered but clock flush failed.
- First checks:
  - check persisted clock path permissions/health,
  - reopen handle and inspect `DbHealthStatus.clock_persist_error`,
  - alert if repeated flush failures persist.
