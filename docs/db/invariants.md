# Wrela DB Invariants

- User key encoding is stable and round-trippable.
- Namespace boundaries are encoded into every logical user key.
- WAL records are append-only and checksum-verified on replay.
- WAL replay is incremental and must tolerate torn tails without replaying partial records.
- Raft durable log state must be contiguous on restore; restore clamps `commit_index <= last_log_index`.
- OCC checks run before mutating storage state.
- Batch apply order is preserved from caller mutation order (same-key ops are not batch-sorted).
- Quorum rejection leaves no queued write residue that can be applied by future requests.
- Read visibility selects latest committed version at or below read version.
- Strong-read rejection checks packed HLC ordering (logical ties included).
- Safe-time observation updates shard, region, and global diagnostics without manual recompute calls.
- Joint membership quorum checks require outgoing and incoming voter majorities while joint config is
  active.
- Residency policy matching canonicalizes region strings (`trim + lowercase`).
- Membership mutation wrappers without auth always fail closed and produce no state side effects.
- `close_db` removes handles from the registry even when flush returns failure.
- Replication conflict handling must monotonically decrease `next_index` until converge-or-fail.
- Persistence health fields set on failure and clear on successful subsequent persist.
