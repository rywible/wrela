# Consensus Fault Suite (Phase 2 Slice)

Integration suite path:

- `/runtime/tests/db_consensus_faults.rs`

Current deterministic scenarios:

- Partition without quorum blocks ACK.
- Partition heal restores ACK path.
- Stale-term follower responses are excluded from quorum.
- Duplicate follower ACK rows do not inflate quorum (unique voter counting).
- Split-vote term followed by next-term winner with more up-to-date log.

Command:

```bash
cargo test -p wrela_runtime --test db_consensus_faults -- --nocapture
```
