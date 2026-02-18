# Wrela DB Invariants

- User key encoding is stable and round-trippable.
- Namespace boundaries are encoded into every logical user key.
- WAL records are append-only and checksum-verified on replay.
- WAL replay is incremental and must tolerate torn tails without replaying partial records.
- OCC checks run before mutating storage state.
- Quorum rejection leaves no queued write residue that can be applied by future requests.
- Read visibility selects latest committed version at or below read version.
- Strong-read rejection checks packed HLC ordering (logical ties included).
- Residency policy matching canonicalizes region strings (`trim + lowercase`).
