# Leader ACK Evaluation Contract (Phase 2 Slice)

## Input

- Required commit position: `(required_term, required_index)`.
- Leader durability flag.
- Follower `AppendEntriesResponse` rows with latency metadata.

## Rule

- A follower counts toward durable quorum only when:
  - `success == true`
  - `response.term >= required_term`
  - `response.match_index >= required_index`
- Quorum counting is per voter (`node_id`), never per response row. Duplicate rows from the same follower count once.
- If multiple responses exist for the same follower, the newest response (higher `term`, then higher
  `match_index`, then `success=true`) is the one used for quorum evaluation.
- Final ACK decision is quorum-based (`(voters/2)+1`) with latency surfaced from durable acks.

## Implementation

- `/runtime/src/db/replication/ack.rs`
- `/runtime/src/db/replication/quorum.rs`
- `/runtime/src/db/mod.rs` (`DbEngine::submit_batch` quorum gate integration)

## Verification

- `cargo test -p wrela_runtime leader_ack_ -- --nocapture`
- `cargo test -p wrela_runtime quorum_ -- --nocapture`
