# Wrela-DB MVCC Audit

## Scope

- `runtime/src/db/mvcc/memtable.rs`
- `runtime/src/db/mvcc/occ.rs`
- `runtime/src/db/mvcc/visibility.rs`

## Current Complexity Profile

- Per-key version history is stored as `Vec<VersionedValue>` under a `BTreeMap`.
- Previous behavior sorted the full version vector on every write (`O(k log k)` per key write under churn).
- Previous visibility lookups scanned full version vectors (`O(k)`).
- Range reads cloned key/value payloads for every returned row.

## Correctness Envelope

- OCC protects against lost updates via expected-version checks.
- Isolation does not guarantee serializable behavior:
  - write skew possible,
  - phantoms possible.
- This envelope is explicitly documented in `mvcc/occ.rs`.

## Write/Read Amplification

- Hot-key write churn caused repeated vector sort work.
- Read path repeatedly scanned version histories instead of direct predecessor lookup.
- Range reads paid avoidable CPU on repeated visibility scans.

## Memory Growth Behavior

- Version chains still grow monotonically over process lifetime.
- Tombstones/old versions are retained without a GC policy.

## Isolation Gaps

- No predicate locking/read-set validation.
- OCC check remains key-local.

## Phased Roadmap

### Near-term (implemented in this pass)

1. Memtable write path:
   - fast append for monotonic versions,
   - binary insertion fallback for out-of-order versions.
2. Memtable read path:
   - `latest_version` now reads last entry in `O(1)`,
   - visible lookup now uses partition-point predecessor search.
3. Visibility helper:
   - switched to partition-point predecessor lookup.

### Mid-term

1. Version-chain GC policy:
   - watermark-based retention for obsolete versions.
2. Range-read optimization:
   - optional borrowed-return iterator path to reduce clone pressure.
3. Version metadata:
   - lightweight per-key chain stats for adaptive cleanup.

### Long-term

1. Optional stronger transactional model:
   - read-set validation / predicate conflict detection.
2. Serializable mode:
   - explicit opt-in with throughput/cost tradeoff.

## Accepted Near-term Changes

- Applied data-structure and lookup-path optimizations that preserve existing semantics while reducing CPU overhead on hot write/read paths.
