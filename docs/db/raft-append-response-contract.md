# Raft Append Response Contract (Phase 2 Slice)

## Message Shape

- `AppendEntriesResponse.term`
- `AppendEntriesResponse.success`
- `AppendEntriesResponse.match_index`
- `AppendEntriesResponse.conflict_index` (optional)

## Leader Progress Rules

- Successful responses update follower progress monotonically.
- Duplicate success responses are idempotent.
- Older-term responses are treated as stale and ignored.
- Failed responses with conflict metadata are recorded as `Conflict` to support deterministic backtracking.
- For leader ACK/quorum evaluation, multiple responses from the same follower are deduplicated to one
  effective row using deterministic precedence: higher `term`, then higher `match_index`, then
  `success=true`.

## Verification

- `cargo test -p wrela_runtime append_tracker_ -- --nocapture`
