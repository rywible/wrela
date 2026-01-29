# Embedded Storage (OpenRaft + RocksDB) Implementation Plan

Date: 2026-01-29

## Goals
- First-class embedded KV storage in the runtime.
- OpenRaft + RocksDB (canonical approach), HTTP transport.
- Async access from Wrela; callers only resolve after durable commit+apply.
- Safe by default: storage client methods must be called on actor instances.
- Single-node mode supported (Raft bootstrap with a single member).

## Scope & Requirements
- KV only (byte keys/values).
- RocksDB only (no pluggable backend initially).
- Internal batching only; no batch API exposed to Wrela.
- Storage service is a singleton per process (one Raft node + one RocksDB).
- Wrela surface should integrate with existing pool semantics.

## High-Level Architecture

### Runtime (Rust)
- **StorageService (singleton):**
  - Owns OpenRaft node + RocksDB state machine + log store.
  - Receives requests over an MPSC channel, batches writes (size/time), and submits Raft proposals.
  - Resolves each caller only after commit + apply.

- **StorageClientActor (poolable):**
  - Actor class used by Wrela.
  - Forwards `get/set/delete` to StorageService and awaits response.

- **HTTP transport:**
  - Implement `RaftNetwork` over HTTP for AppendEntries/InstallSnapshot/Vote.

### Wrela API
- Prefer explicit pool semantics (primary usage):
  - `db = detach Pool.of(StorageClient, size=8, backpressure=queue(128)) * 1`
  - `await db.set("k", "v")`
  - `val = await db.get("k")`

- Optional convenience (sugar only):
  - `storage.open()` -> returns a pooled actor with size=1.

## Safety & Compile-Time Guardrails
- We added a conservative **requires-actor** analysis in the compiler:
  - Any class/method that (directly or indirectly) uses `await` or `fire` is actor-only.
  - Errors include a call-chain hint to the await.
- StorageClient should rely on this system to prevent sync instantiation/method calls.

## Configuration
All config is read at runtime (env vars), with programmatic overrides if needed later.

- `WRELA_STORE_ENABLED` (bool, default false)
- `WRELA_STORE_PATH` (string, default: `./wrela.db`)
- `WRELA_RAFT_NODE_ID` (u64, default 1)
- `WRELA_RAFT_BIND_ADDR` (string, default `127.0.0.1:8080`)
- `WRELA_RAFT_PEERS` (csv `id=addr`, default empty)
- `WRELA_RAFT_BOOTSTRAP` (bool, default true for single-node)
- `WRELA_RAFT_SNAPSHOT_INTERVAL` (entries, default 10_000)
- `WRELA_STORE_BATCH_MAX_OPS` (usize, default 128)
- `WRELA_STORE_BATCH_MAX_MS` (u64, default 5)

### Single-node mode
- `WRELA_RAFT_PEERS` empty and `WRELA_RAFT_BOOTSTRAP=1`.
- Initializes membership with a single node.
- Later multi-node expansion via membership changes.

## Implementation Steps

### 1) Add dependencies
- `openraft`
- `openraft-rocksstore` (or copy from `raft-kv-rocksdb` example)
- HTTP transport: `reqwest` + `axum`/`hyper` (or minimal custom HTTP server)

Decision: start with `openraft-rocksstore` or adapt `raft-kv-rocksdb` for full control.

### 2) Create runtime modules
Suggested module layout:
```
crates/runtime/src/storage/
  mod.rs
  config.rs
  service.rs
  client.rs
  transport.rs
  store.rs   (rocksdb + raft storage impl)
```

### 3) StorageService
- Initialize RocksDB + Raft storage.
- Initialize OpenRaft node and HTTP server.
- Start background batching loop:
  - Collect `Put/Del` ops into batch (max ops or time).
  - Submit one Raft proposal per batch.
  - Resolve per-op oneshots once the batch is committed + applied.
- Reads:
  - `Leader` read (linearizable) via Raft (default).
  - Optional `Local` read (stale, fast) for internal use.

### 4) StorageClientActor
- Actor methods: `get`, `set`, `delete`.
- Send request to StorageService via channel + await reply.
- Error handling: queue overflow -> return error result.

### 5) Wrela runtime bindings
- Add builtins for:
  - `storage_open()` (returns pooled actor handle size=1) [optional]
- Provide class `StorageClient` in stdlib (wrapping actor methods).

### 6) HTTP Transport
- Implement Raft RPC handlers:
  - `append_entries`
  - `install_snapshot`
  - `vote`
- Implement client side in `RaftNetwork`:
  - map NodeId -> address
  - send JSON/protobuf

### 7) Bootstrapping
- On startup:
  - if `BOOTSTRAP` true and storage is empty:
    - call `raft.initialize(membership)` with self only (single-node)
  - if peers provided:
    - require explicit bootstrap when forming a new cluster
    - otherwise join existing leader

### 8) Metrics + diagnostics
- Add counters:
  - batch size
  - batch latency
  - commit latency
  - read latency
- Wire into existing `metrics.rs` and `diagnostics.rs`.

### 9) Tests
- **Unit:** state machine apply, log persistence, snapshot load.
- **Integration:**
  - single-node durability across restart
  - multi-node replication under restart
  - snapshot compaction

## Risks / Decisions
- **Transport:** choose minimal HTTP stack to keep dependencies light.
- **Batching:** too aggressive batching can hurt latency; defaults should be small.
- **Consistency:** default reads should be leader-linearizable.
- **API clarity:** recommend pool usage in docs to make actor semantics obvious.

## Milestones
1. Skeleton modules + config + StorageService stub
2. OpenRaft + RocksDB wiring (single-node)
3. Wrela-facing API (StorageClient actor + pool usage)
4. HTTP transport + multi-node join
5. Tests + metrics

