# Wrela-DB Serialization Audit (Embedded + Private RPC)

## Scope

- `runtime/src/db/rpc/private_network.rs`
- `runtime/src/db/raft/persistence.rs`
- `runtime/src/db/topology/persistence.rs`
- `runtime/src/db/wal/format.rs`
- `runtime/src/db/wal/segment.rs`

## Ranked Recommendations

### R1: Low-risk immediate

1. Remove transient allocation churn from WAL encode/checksum:
   - No more `concat()` checksum buffers in encode/decode.
   - Added append-in-place WAL encoding (`encode_to`) for batch writes.
2. Reduce JSON persistence allocation overhead:
   - Switched raft/topology load to `serde_json::from_reader`.
   - Switched raft/topology persist to `serde_json::to_writer` (no `to_vec` staging).

Status: implemented.

### R2: Medium-risk

1. Add optional binary private RPC wire format for embedded cluster traffic.
2. Keep rollback switch at runtime config level:
   - `WRELADB_PRIVATE_RPC_WIRE_FORMAT=json|binary`
3. Preserve compatibility:
   - Reader accepts tagged frames and legacy untagged JSON frames.

Status: implemented with rollback switch.

### R3: Higher-risk

1. Binary persistence format migration for raft/topology state.
2. Dual-read migration strategy:
   - Prefer new binary files.
   - Fallback to JSON.
3. Dual-write grace period, then remove JSON writes after confidence window.

Status: deferred.

## Implemented Wins

- WAL encoding now avoids temporary concatenated payload allocations.
- WAL batch append now preallocates and reuses a single output buffer.
- WAL replay checksum verification now operates on slices before owned copies.
- Raft/topology persistence moved from JSON `Vec<u8>` staging to streaming reader/writer APIs.
- Private RPC now supports binary framing with a rollback environment switch.

## Deferred Items

- Binary on-disk persistence migration (raft/topology) left deferred to avoid compatibility risk in the same performance pass.
- Cross-version mixed-cluster compatibility policy for persistence format transition remains to be specified.
