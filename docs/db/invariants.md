# Wrela DB Invariants

- User key encoding is stable and round-trippable.
- Namespace boundaries are encoded into every logical user key.
- WAL records are append-only and checksum-verified on replay.
- OCC checks run before mutating storage state.
- Read visibility selects latest committed version at or below read version.

