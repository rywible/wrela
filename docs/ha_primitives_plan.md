# HA Primitives Plan (Single-Cluster, 3-Node)

This document describes how to make all runtime primitives HA while retaining a single-node mode. It targets a single Wrela cluster (3 nodes), low latency, high throughput. The approach leverages the existing Raft-backed storage as the source of truth and introduces a lightweight intra-cluster pub/sub for fanout.

## Goals

- All primitives are HA in a 3-node cluster.
- Single-node mode remains fully supported.
- Low latency and high throughput.
- Minimal additional infrastructure; reuse Raft storage where possible.
- Clear failure semantics (no duplicates, or explicit at-least-once where unavoidable).

## Principles

1. **Storage is source of truth**
   All durable state and coordination (leases, schedules, queue state, membership) is persisted in the Raft-backed storage.

2. **Leasing for work ownership**
   All distributed workers use leases with TTL and renewal for safe failover.

3. **Linearizable reads for correctness**
   Reads that affect correctness (leases, job claims, schedule runs, auth/rbac checks, rate limits) must be linearizable.
   Stale/local reads are only allowed for soft UX surfaces (e.g., admin dashboards, non-critical hints).

4. **Idempotency where at-least-once**
   If at-least-once delivery is used, store idempotency keys and last-processed markers.

5. **Single-node mode**
   All HA features degrade to single-process behavior if the cluster is size 1.

6. **Avoid hot leader bottlenecks**
   Writes go through leader (Raft), but read and fanout should be local when possible.

## Shared Building Blocks

### 1) Leases

- **Key format**: `lease:<namespace>:<resource>`
- **Value**: `{ owner_id, exp_epoch, term }`
- **Acquire**:
  - `get_with_version` → if missing or expired, `set_if_version` to claim
- **Renew**:
  - `set_if_version` with same key and current version
- **Release**:
  - `delete_if_version`

**Requirements:**
- Storage must support `get_with_version`, `set_if_version`, and `delete_if_version` (already added).

### 2) Atomic batches

- Use `batch_set` for atomic metadata updates (ex: schedule + run state in one commit).

### 3) Pub/Sub (lightweight)

- Introduce a minimal, in-cluster pub/sub:
  - Backed by storage for membership and routing
  - UDP or HTTP fanout between nodes
  - Best effort; delivery is not strictly guaranteed (state is still in storage)
- Use for:
  - Realtime room fanout
  - Job wakeups (optional)
  - Scheduler tick notifications (optional)

## Primitive-by-Primitive HA Design

### A) Scheduler (cron/at/every)

**Current:** in-process timers only, no rehydrate.

**HA Design:**
- Persist schedules and next-run timestamp in storage.
- Leader-only scheduling:
  - Leader holds a lease `lease:scheduler:leader`.
  - Leader scans `schedule:entries` and triggers due tasks.
- **Run tracking:**
  - For each scheduled execution, create a `schedule:run:<id>` record.
  - If already exists, skip (prevents duplicate execution on failover).

**Single-node mode:**
- Same logic, no lease contention; local node always leader.

### B) Jobs

**Current:** queue stored, processing local.

**HA Design:**
- Jobs are stored in storage as today.
- Add per-job leases:
  - `jobs:lease:<queue>:<job_id>`
- Workers claim by lease (compare-and-set with version).
- Processing uses at-least-once semantics with idempotency key:
  - `jobs:done:<queue>:<job_id>`
  - If exists, skip.
- DLQ remains stored as today.

**Single-node mode:**
- Lease claim always succeeds locally; no extra overhead.

### C) Realtime

**Current:** in-memory rooms + inbox.

**HA Design:**
- Room membership persisted:
  - `realtime:room:<room_id>` → list of socket IDs
  - `realtime:socket:<socket_id>` → node + metadata
- Fanout:
  - Node receiving event writes to storage and publishes to pub/sub.
  - All nodes with room members deliver locally.
- Failure handling:
  - On disconnect, remove from storage.
  - On node failover, stale sockets are cleaned by TTL or heartbeat.

**Single-node mode:**
- Works entirely in-memory with storage as optional durability.

### D) Files

**Current:** stored in storage as base64, synthetic signed URL.

**HA Design:**
- Store metadata + ACL in storage.
- Bytes in S3 (or S3-compatible) with multipart streaming.
- Signed URL generation:
  - Use S3 presigned URLs.
  - Validate ACL + owner in storage before issuing.
- Optional proxy endpoint for private access:
  - Node validates token + ACL, then fetches from S3 or redirects.

**Single-node mode:**
- Use local disk backend with same metadata model.

### E) Search

**Current:** naive scan.

**HA Design (Phase 1):**
- Persist inverted index in storage:
  - `search:term:<collection>:<token>` → list of doc IDs
- Document store remains in storage.
- Queries read index from storage, then fetch docs.

**Phase 2 (optional):**
- External search cluster (OpenSearch/Meilisearch) if scale demands.

**Single-node mode:**
- Same logic using local storage.

### F) Auth / RBAC / Rate Limit / Admin

- Already storage-backed. Needs:
  - Leader-safe writes (Raft)
  - No extra HA work except making sure tokens/ratelimits use storage consistently

## Cluster Coordination

- **Leader election** via storage lease or Raft leader.
- **Node identity**: each node has `node_id` and `bind_addr`.
- **Membership** stored in Raft config.

## Consistency & Failure Semantics

- Scheduler: at-most-once per run ID (dedupe).
- Jobs: at-least-once with idempotency marker.
- Realtime: best-effort; storage ensures membership but delivery may drop if node fails mid-send.
- Files: consistent via storage metadata + S3 durability.
- Search: eventually consistent; index updates are transactional via batch set.

## Implementation Steps

1) Add lease helper APIs in runtime
2) Scheduler rehydrate + leader-lease + run-id dedupe
3) Jobs lease-based claiming + idempotency key
4) Realtime: storage-backed membership + pub/sub fanout
5) Files: S3 integration for bytes + ACL validation
6) Search: storage-backed inverted index
7) Add HA test suite (multi-node integration tests)

## Testing Plan

- Multi-node tests for:
  - Leader failover with in-flight scheduler and jobs
  - Duplicate suppression for schedule runs
  - Job lease recovery after node kill
  - Realtime fanout across nodes
  - Search index consistency after crash
- Single-node tests for all primitives remain as today.

## Notes on Single-Node Mode

- Cluster size 1 works with the exact same logic; leases always acquired locally.
- Pub/sub can be disabled in single-node mode (direct in-process delivery).
