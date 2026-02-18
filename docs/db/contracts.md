# Wrela DB Contracts

## Correctness

- `ACK` is emitted only after durable WAL append in the local kernel path.
- `submit_batch` is caller-atomic: no partial apply is visible on WAL write/sync errors.
- Raft durable state (`term`, `vote`, `commit_index`, contiguous log tail, membership/joint config)
  is persisted in `raft_state.json`; malformed/corrupt state fails open-path closed with `Io`.
- Keyspace is namespace-scoped and deterministic.
- `expected_version` mismatches fail deterministically.
- Quorum-rejected batches must not leak into subsequent accepted batches.
- Strong reads compare full HLC ordering (`physical + logical`), not physical time only.
- Public `get`/`scan` are strong-by-default; eventual reads are explicit (`ReadConsistency::Eventual`).
- CDC checkpoint ack uses copy-on-write persistence; in-memory checkpoint does not advance if persist
  fails.
- CDC checkpoint durability is fsync-hard (`temp write -> sync_data -> rename -> parent-dir fsync`).
- Membership mutations are authenticated-only (`RpcClass::ClusterAdmin`); compatibility wrappers fail
  closed with `UNAUTHORIZED_MEMBERSHIP_MUTATION`.
- `close_db` always unregisters the handle, even when clock flush fails (returns `false`).

## Limits

- `key <= 1 KiB`
- `value <= 1 MiB`
- `batch <= 4 MiB`
- `batch ops <= 1024`

## Availability

- Single-node kernel path is deterministic and crash-recoverable from WAL replay.
- Replicated append path enforces `prev_log_index/prev_log_term` log-matching and conflict-index
  rejection semantics before accepting append payloads.
- Follower replication convergence is bounded by `leader.last_log_index + 2` attempts and requires
  monotonic `next_index` progress; no-progress exits with retryable typed failure.
- Joint membership writes require dual durable quorum (outgoing + incoming voter sets).
- Admin health surfaces persistence failures (`clock`, `raft`, `cdc checkpoint`) and clears each
  field after a successful subsequent persist.
