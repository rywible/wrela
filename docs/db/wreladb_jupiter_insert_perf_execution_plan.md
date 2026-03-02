# Wrela-DB Jupiter Insert Performance Execution Plan (Parallelized)

Date: February 23, 2026  
Owner: Wrela Runtime Team (Replication/Protocol, Storage/LSM, Perf/Infra)

## Full-Send Cutover Delta (February 24, 2026)

This branch now runs Jupiter features as hard runtime defaults instead of per-feature env toggles:

1. Outside-lock replication and WAL encode-outside-lock are always enabled.
2. Sorted-run catch-up sender/receiver path is always enabled with fixed lag/chunk defaults.
3. Compaction scheduler and blob value-separation/GC paths are always enabled with fixed defaults.
4. Insert fast-lane and frontier planning are always enabled; eligibility/correctness guards remain enforced.
5. `ReplicatedLogBackend::CanonicalOnly` is the default runtime backend; `DualWal` and `ShadowCanonical` remain for parity/rollback validation.
6. Docs and tests are being moved to "always-on + robust matrix default" semantics.

## Task-Manager Execution Board (JUP-###)

Execution is tracked by `JUP-###` tasks with strict dependency edges and blocking validation per task.

Completed:
1. `JUP-101` / `JUP-102`: outside-lock replication and WAL encode-outside-lock are unconditional defaults.
2. `JUP-201` / `JUP-202`: canonical backend behavior is active and parity/rollback validation passes across `DualWal`, `ShadowCanonical`, and `CanonicalOnly`.
3. `JUP-301` / `JUP-302`: sorted-run catch-up sender/receiver convergence paths are active and hardened by rejection/retry tests.
4. `JUP-401` / `JUP-402` / `JUP-403` / `JUP-404`: value separation is wired through runtime write/read/iterator/CDC with backward decode compatibility and GC safety tests.
5. `JUP-501` / `JUP-502`: insert fast-lane eligibility/fallback behavior is always-on with counter correctness coverage.
6. `JUP-601` / `JUP-602`: frontier planner path is default behavior with perf/health telemetry validation.
7. `JUP-801` / `JUP-802` / `JUP-803`: strict blocking matrix + fly evidence workflows are default-on.

## Final Closure Snapshot (February 24, 2026)

Blocking matrix command:

```bash
scripts/perf/jupiter_integration_blocking_matrix.sh --sha 68108003d298959ae0be2848c1fd82d5e37e0495
```

Result:
1. `cargo test -p wrela_runtime --lib`: pass.
2. Core cluster correctness pack: pass.
3. Strict perf evidence (baseline + meso + sweep): pass and indexed under `.artifacts/perf/local-db-write/`.
4. Fly drills: pass.
   - smoke: `artifacts/fly/wrela-smoke-20260224121731-smoke-report.json`
   - write load: `artifacts/fly/wrela-load-20260224121938-write-load-report.json`
   - cluster drill: `artifacts/fly/wreladb-lab-20260224122453-drill-report.md`
   - chaos loop (full run): `artifacts/fly/wreladb-lab-20260224122821-chaos-20260224122853/summary.md` (`20/20` passed)

## Implementation Push Delta (February 23, 2026)

This execution wave landed the core public surface + safety plumbing for the "one massive parallel push":

1. `E11` hot-path allocation wins shipped in runtime write path:
   - `WriteEnvelope::Put` now carries `Bytes` directly.
   - redundant WAL clone removed in lane submit path (`std::mem::take`).
   - memtable upgraded to `SmallVec<[VersionedValue; 1]>` chains with existing-key `get_mut` fast path.
2. `E3` backend surface expanded:
   - `ReplicatedLogBackend::CanonicalOnly` added and wired through options/health/perf schema handling.
3. `E4` sorted-run catch-up receiver semantics now landed beyond surface:
   - proto messages + `ReplicaInstallSortedRunChunk` RPC endpoint added.
   - tonic/private-network wiring now routes into runtime install logic (not hardcoded fail-closed).
   - receiver validates stale term rejection, out-of-order chunk rejection, and duplicate replay safety.
   - convergence apply is implemented: chunk stream completion decodes deterministic SST blocks and applies to memtable.
4. `E6` + `E7` starter modules and telemetry landed:
   - new `lsm/scheduler.rs` compaction admission logic + tests.
   - new `lsm/blob_store.rs` threshold value-separation/blob-GC primitives + tests.
   - SST encode/decode now supports additive blob-reference value encoding with backward-compatible decode for legacy blocks.
   - health status now exports scheduler/blob metrics and controls.
5. `E8` authorized insert-fast lane landed:
   - new authz class `ClientInsertFast`.
   - new API entrypoints for authorized fast put/batch.
   - eligibility/fallback path with accepted/rejected telemetry counters.
6. `E10` frontier-mode control + telemetry landed:
   - speculative/wave planning counters wired in replication hot path.
7. Jupiter runtime behavior now runs as hard defaults (outside-lock replication, WAL encode-outside-lock, sorted-run sender/receiver, compaction scheduler, value separation/GC, insert fast-lane, frontier planning).
8. `E2a` lock-scope phase split is now wired into the writer lane for private-mesh quorum writes:
   - explicit prepare-under-lock / replicate-outside-lock / finalize-under-lock flow added.
   - network fanout now executes outside the `DbEngine` mutex on the default path.
   - direct split-path unit coverage added (`outside_lock_prepare_replicate_finalize_roundtrip_preserves_quorum_visibility`).
9. `E2a` non-mesh fallback path is now also lock-scope split:
   - when private mesh is unavailable, quorum simulation fanout executes outside the mutex and hands follower-state updates back to finalize-under-lock.
   - strict failure semantics for `RequirePrivateRpc` remain intact in outside-lock mode.
10. `E4` sender-side sorted-run selection is now wired in private-mesh fanout:
   - lag-threshold planner (`TailLogOnly` vs `SortedRunThenTail`) is active with fixed runtime defaults.
   - deterministic SST chunk payload builder and sender RPC loop are integrated before tail-log replication on selected targets.
   - sender telemetry now increments `sorted_run_catchup_chunks_sent`.

Still open for follow-on pushes:
1. Expand mixed-topology/fault coverage for `E2a` outside-lock paths (mesh + no-mesh) and run strict perf retuning sweep.
2. Add deeper sender correctness drills for sorted-run catch-up rejection/retry behavior in same-version clusters under coordinated cutover.
3. End-to-end value-separation runtime read/write path integration beyond SST blob-ref compatibility (blob indirection wiring in compaction/reads).

## 1) Mission and Non-Negotiables

### Mission
Build a write path that feels unfairly fast while preserving strict durability semantics:

- Default commit rule: ACK only after quorum has durably persisted log record(s).
- No silent durability downgrade in production.
- Async apply is allowed for latency, but strong reads must obey apply-visible index.

### Non-Negotiables
1. `language/spec` is an executable spec project and should remain green in check/spec-lane test workflows.
2. Jupiter runtime behavior ships as always-on defaults; rollback uses backend compatibility paths and coordinated cutover procedures, not per-feature env toggles.
3. No perf claim without reproducible artifact evidence.
4. No temporary correctness hacks in mainline.
5. No gate claims from debug-only runs.

## 2) Program Gates (Unchanged)

1. Gate A: >= 2.0x throughput at equal-or-better p99 on local insert-only harness.
2. Gate B: >= 30% p95 commit latency reduction on single-shard insert workload.
3. Gate C: >= 25% p99.9 reduction under compaction/GC stress workload.
4. Gate D: replication latency components are non-synthetic and queue depth remains bounded under sustained load.
5. Gate E: crash/fault tests pass for quorum-ACK-before-apply and replay convergence.

## 3) Current Status Snapshot (Cleaned)

### Completed and Removed from Active Backlog
1. Commit visibility surfaces exist and are wired (`durability_commit_index`, `apply_visible_index`, apply backlog status).
2. Local perf harness has stage telemetry, tail metrics, retry-after breakdown, queue/backlog telemetry.
3. Strict anti-cheat schema checks are in place and enforced.
4. Strict real-quorum local harness mode exists and passes with:
- `simulation_commits = 0`
- `real_quorum_evidence = true`
- private-RPC quorum transport in effect.
5. Replication RPC backpressure telemetry is exposed (in-flight cap, available permits, timeout/closed counters).
6. Shadow single-log telemetry path exists and is schema-validated (anti-cheat consistency checks).
7. Local strict evidence ledger exists (`.artifacts/perf/local-db-write/INDEX.jsonl`) and is auto-populated by baseline + meso scripts.
8. Replication telemetry now includes batch byte-size distribution and wave-shape metrics (avg/max wave targets), wired through harness artifacts.
9. Runtime surfaces explicit hard-failure tokening for disallowed simulation fallback paths.
10. Replication fanout outcome telemetry is now exposed (success/fail/cancel counts) and schema-enforced in perf artifacts.
11. Writer-lane skew aggregates are exposed as basis points in health status and surfaced in perf artifacts.
12. Claimability gate script exists and is wired (`assert_local_db_write_claimable.sh`) to fail-closed evidence claims.
13. Strict evidence assertions now require positive successful fanout in every workload (`replication.successful_count > 0`).
14. Replication fanout hot path no longer clones per-wave target slices before spawning RPC tasks.
15. Replication efficiency telemetry now includes `contact_efficiency_bps` and `target_efficiency_bps`.
16. Writer lane pool now performs adaptive shard-to-lane assignment (least-assigned lane with stable per-shard mapping), preserving per-shard ordering.
17. Replication sender now pre-encodes protobuf batch payload once per commit wave template and reuses it per replica RPC.
18. Writer-lane assignment telemetry is now end-to-end (`lookups/hits/misses/hit_rate_bps`) across health + perf artifacts.
19. Perf claimability gate now supports fail-closed lane-spread enforcement for multi-lane runs (`--require-lane-spread 1`).
20. Strict, indexed local artifacts validated with lane-spread evidence on `WRELADB_LOGICAL_SHARDS=8` and `WRELADB_WRITER_LANE_COUNT=2`.
21. Harness-level anti-cheat now fail-closes multi-lane runs when `WRELA_LOCAL_PERF_REQUIRE_LANE_SPREAD=1` and shard spread is not exercised.
22. Replication fanout now keeps voter-to-mesh-node mapping deterministic regardless of dynamic priority ranking.
23. Writer lane batch assembly now uses linear queue partitioning instead of repeated indexed removal scans under lock.
24. Replication RPC permit acquisition now uses a short configurable timeout (`WRELADB_REPLICATION_RPC_PERMIT_TIMEOUT_MS`, default 25ms) instead of full I/O timeout.
25. Fanout wave processing now short-circuits immediately after enough successful RPCs in-wave to satisfy additional quorum needs.
26. Strict profile sweep runner now exists (`scripts/perf/local_db_write_profile_sweep.sh`) with schema/evidence/index/claimability gates per profile.
27. Replica-local follower writes now run through the writer-lane pipeline (with `ReplicationCommitMode::ReplicaLocal`) instead of the old direct synchronous apply path.
28. Replication idempotency tokens now include `active_group_id`, eliminating cross-group token collisions that caused `IDEMPOTENCY_TOKEN_REUSE_MISMATCH` under parallel fanout.
29. Quorum follower error telemetry now classifies private-RPC failures by tokenized cause (for example `QUORUM_PRIVATE_RPC_INVALID_ARGUMENT`) instead of collapsing everything into generic durability misses.
30. Local harness now records top retry-after message samples (`retry_after_messages`) to expose concrete retry roots in artifacts instead of cause-token-only summaries.
31. Apply lane now supports batched task drains per lock acquisition (`WRELADB_APPLY_LANE_BATCH_MAX`, default `64`) to reduce apply lock churn.
32. Apply execution now supports shard-group-routed apply lane pools (`WRELADB_APPLY_LANE_COUNT`, defaulting to writer lane count) with deterministic group-to-lane routing.
33. Health surfaces now export per-apply-lane telemetry (`queue_depth`, `enqueue_attempts`, `max_queue_depth`, `dequeued_tasks`) and aggregate max queue depth.
34. Local perf harness artifacts now include `apply_lanes` telemetry; schema gate scripts fail closed if this surface drifts.
35. Private-RPC transport now supports configurable multi-channel-per-target pooling (`WRELADB_PRIVATE_RPC_CHANNELS_PER_TARGET`) with channel selection spread; default remains conservative (`1`).

### Course Correction (February 23 Review)

Prior work delivered strong telemetry, evidence infrastructure, and replication correctness.
However, the core write-path critical section was not structurally addressed. Specifically:

1. `replicate_entries_for_quorum` performs network I/O (`block_on_runtime`) while holding
   the `DbEngine` mutex (`prepare_and_apply_batch` at `mod.rs:4237`). Harness stage
   breakdowns show ~64% of write latency is replication time. During that window every
   other write lane is blocked, making lane parallelism (E5) and batching knobs (E2)
   unable to deliver their intended throughput gains.
2. WAL encoding (CRC32 + byte serialization) runs under the engine lock (`mod.rs:4267-4287`)
   with no engine-state dependency after version assignment.
3. The memtable (`BTreeMap<Vec<u8>, Vec<VersionedValue>>`) has no epic and leaves
   per-insert key allocations and poor cache locality unaddressed.
4. Hot-path allocation waste (`cache_key = user_key.clone()`, `Vec<u8>` to `Bytes` clone
   chains, per-op `encode_user_key` allocs) inflates the critical section.

New epics `E2a` (lock-scope reduction) and `E11` (memtable + hot-path allocations) are
added below. `E2a` is the single highest-leverage item for Gate A and Gate B and must
land before E2/E5 knob tuning produces meaningful signal.

### Current Completion State
1. All Jupiter epics in this wave are code-complete and running as default runtime behavior (no per-feature toggles).
2. Canonical durability backend is active and validated against rollback/compat parity tests.
3. Sorted-run sender/receiver catch-up is active with stale-term/duplicate/out-of-order/restart convergence coverage.
4. Value-separation + blob-GC runtime wiring is complete across write/read/iterator/CDC, including decode compatibility.
5. Fly hardening drills and strict perf evidence are integrated into the blocking matrix and passed for this wave.

## 4) Strict-Only Perf Policy (Now Mandatory)

From this point forward, decision-making evidence must be strict mode.

### Required Defaults
1. Local perf harness always runs strict real-quorum mode (no env override path).
2. Strict evidence assertions are mandatory for every local perf artifact.

### Allowed Exceptions
1. None for local perf gate artifacts.

### Enforced Scripts
1. `scripts/perf/local_db_write_baseline_capture.sh` defaults strict mode.
2. `scripts/perf/local_db_write_meso_compare.sh` runs strict-only by default (control is opt-in via `--with-control`).

## 5) Epic Status Board (Code-Complete for This Wave)

1. `E0` Perf truth infrastructure + telemetry hardening: **100% code-complete**
2. `E1` Real quorum replication pipeline: **100% code-complete**
3. `E2a` Engine lock-scope reduction: **100% code-complete**
4. `E2` Commit conveyor + replication micro-batching + backpressure: **100% code-complete**
5. `E3` Single durable log architecture: **100% code-complete**
6. `E4` Sorted-run replication: **100% code-complete**
7. `E5` Per-core writer/ingest lanes: **100% code-complete**
8. `E6` Compaction exile + SLA scheduler: **100% code-complete**
9. `E7` Value separation + blob GC: **100% code-complete**
10. `E8` Insert-rights fast lane: **100% code-complete**
11. `E9` Fly production hardening: **100% code-complete**
12. `E10` Frontier track: **100% code-complete**
13. `E11` Memtable + hot-path allocation optimization: **100% code-complete**

Gate note:
1. This closure is a code-complete + flag-safe (now default-on) ship bar.
2. Perf gate recovery remains tracked by strict evidence artifacts and can continue iteratively without reopening feature completeness.

## 6) Dependency Graph (What Blocks What)

### Hard Dependencies
1. `E0` -> blocks all gate claims (A-E).
2. `E1` -> blocks meaningful `E2` tuning and any production-like throughput claims.
3. **`E2a` -> blocks `E2` knob tuning and `E5` lane parallelism from producing real throughput gains.** While the engine lock holds for replication network I/O, adding lanes or batching knobs measures noise, not signal.
4. `E2a` + `E2` + `E5` -> block clean `E3` cutover evaluation (otherwise signal is noisy).
5. `E3` -> blocks `E4` correctness/perf validation for canonical apply path.
6. `E6` + `E7` -> block Gate C.
7. `E9` -> blocks production rollout regardless of benchmark wins.

### Critical Path to Gate A (2x throughput)
```
E1 (quorum correctness) -> E2a (lock-scope reduction) -> E2/E5 (knob tuning + lanes) -> Gate A
```
`E2a` is the structural prerequisite. `E1` closure provides the correctness confidence needed to restructure the replication call site safely.

### Can Proceed in Parallel (No Hard Block)
1. `E0` remaining CI work can finish in parallel with everything.
2. `E1` closure and `E2a` implementation can overlap (E2a design starts now, E1 fault matrix validates the new call pattern).
3. `E11` (memtable + hot-path allocs) can proceed independently of all other epics.
4. `E3` shadow verification can run in parallel with `E2a/E2/E5`.
5. `E6` scheduler scaffolding can begin before `E4` completes.
6. `E7` metadata/schema design can begin before full `E6` rollout.
7. `E9` operational/runbook work can proceed in parallel with all engineering epics.

## 7) Parallel Workstreams (Massively Parallel Execution)

## WS-LOCK: Engine Lock-Scope Reduction (`E2a`) -- HIGHEST PRIORITY
Owner: Replication/Protocol

This is the single highest-leverage workstream. `replicate_entries_for_quorum` currently
runs network I/O via `block_on_runtime` while holding the `DbEngine` mutex. All other
write lanes stall during replication (~64% of write latency). Fixing this is the
structural prerequisite for Gate A and Gate B.

### Run Now
1. **Move replication outside the engine lock.** Restructure `prepare_and_apply_batch` into:
   - Phase 1 (under lock): version assignment, Raft log append, memtable apply (StrictApply), capture replication payload + entries. Return a `ReplicationTask` struct.
   - Phase 2 (lock released): execute `replicate_entries_for_quorum` against the captured payload.
   - Phase 3 (re-acquire lock or lock-free): mark durable, advance commit index, dispatch apply tasks.
2. **Move WAL encoding outside the engine lock.** After version assignment, WAL record encoding (CRC32 + byte serialization at `mod.rs:4267-4287`) has no engine-state dependency. Capture staged records under lock, encode after release.
3. Keep replication-outside-lock as the default runtime path; rollback uses backend compatibility and coordinated cutover, not per-feature toggles.
4. Validate correctness: replication-before-WAL ordering, crash recovery, replay convergence.
5. Strict meso comparison: before/after lock-scope reduction with identical workload.

### Blocked By
1. `E1` fault matrix confidence (needed to validate the restructured replication call site).
2. Nothing else. Design and implementation can start immediately.

### Blocks
1. `E2` knob tuning producing meaningful signal.
2. `E5` lane parallelism delivering real throughput gains.
3. Gate A (2x throughput) and Gate B (30% p95 reduction).

## WS-A: Perf Truth + Enforcement (`E0`)
Owner: Perf/Infra

### Run Now
1. Finalize strict-only local baseline policy docs and script outputs. **(done)**
2. Add CI check that fails if perf-tagged PR evidence is missing strict-run marker. (lower priority than WS-LOCK)
3. Add artifact bundle index generation (commands, SHA, artifact path, verdict). **(done)**

### Blocked By
1. Nothing.

### Blocks
1. Any valid gate claim.

## WS-B: Real Quorum Pipeline Closure (`E1`)
Owner: Replication/Protocol

### Run Now
1. Remove remaining synthetic/local follower simulation from non-test hot paths.
2. Ensure quorum ack reason taxonomy is complete and stable.
3. Expand private-RPC harness and chaos scenarios for follower lag/fsync failure token coverage.
4. **Validate that the E2a lock-restructure preserves quorum-ACK-before-visibility semantics under fault injection.**

### Blocked By
1. None for implementation.
2. Phase exit blocked by strict evidence bundle + fault matrix pass.

### Blocks
1. Trustworthy `E2a` lock restructure (correctness confidence).
2. Trustworthy `E2`/`E5` optimization decisions.

## WS-C: Commit Conveyor + Lane Parallelism (`E2` + `E5`)
Owner: Replication/Protocol + Perf/Infra

**Note:** Knob tuning and meso sweeps are deferred until `E2a` (lock-scope reduction) lands. Before that, lane parallelism and batching knobs measure contention noise, not real batching behavior.

### Run Now (pre-E2a)
1. Implement `replication_batch_max_ops` and `replication_batch_max_bytes` knob surfaces (code-ready, tuning deferred).
2. Heartbeat/write coalescing implementation (code-ready, telemetry validation deferred).

### Run After E2a Lands
1. Strict meso sweeps for knob tuning matrix (now measuring real signal).
2. Lane skew telemetry + auto-balance heuristics.
3. Selected default profile based on strict evidence.

### Blocked By
1. `E2a` for meaningful tuning signal.
2. `E1` closure for final signoff quality.

### Blocks
1. Clean `E3` cutover evaluation.

## WS-ALLOC: Memtable + Hot-Path Allocation Optimization (`E11`) -- NEW
Owner: Storage/LSM + Perf/Infra

Can proceed independently and in parallel with all other workstreams.

### Run Now
1. **Eliminate `cache_key = user_key.clone()`** at `mod.rs:4176` (identical to `user_key`, pure waste).
2. **Eliminate triple-clone in shadow_versions**: `user_key` is cloned for `cache_key`, then cloned again for `shadow_versions.insert`. Use shared `Bytes` or a single owned value.
3. **Store `Bytes` in `WriteEnvelope`** instead of `Vec<u8>` so the `BatchOp` conversion at `mod.rs:2846-2851` is a cheap Arc bump instead of clone + convert.
4. **Avoid memtable key clone on existing keys**: use `get_mut` before `entry()` in `Memtable::apply` to skip the key allocation when the key already exists.
5. **Eliminate `wal_bytes.clone()`** at `mod.rs:2961`: take ownership on WAL submit instead of cloning.
6. Evaluate `SmallVec<[VersionedValue; 1]>` for version chains (common single-version case).
7. Evaluate replacing `BTreeMap` with a concurrent skip list or ART for better cache locality.

### Blocked By
1. Nothing.

### Blocks
1. Nothing directly, but reduces critical-section time which compounds with E2a.

## WS-D: Single Durable Log Cutover (`E3`)
Owner: Replication/Protocol

### Run Now
1. Define canonical log record format and recovery contract.
2. Keep shadow mode running in strict perf runs.
3. Build replay parity test harness old-vs-new.

### Blocked By
1. Full cutover signoff is blocked on stable `E2a/E2/E5` path.

### Blocks
1. `E4` sorted-run replication becoming production-viable.

## WS-E: Storage Pressure Killers (`E6` + `E7`)
Owner: Storage/LSM

### Run Now
1. Implement compaction budget/admission scaffolding.
2. Add compaction debt and stall telemetry surfaces.
3. Start value-separation file format + pointer metadata design.
4. Add GC accounting primitives (rewrite bytes, reclaimed bytes, space amp estimate).

### Blocked By
1. None for scaffolding.
2. Gate C signoff blocked by full workload + stress proof.

### Blocks
1. Long-term tail latency stability.

## WS-F: Production Hardening (`E9`)
Owner: Perf/Infra

### Run Now
1. Define strict canary checklist and rollback automation contract.
2. Add health SLO alert definitions tied to queue/backlog/retry-after signals.
3. Document Fly private mesh operational runbooks.

### Blocked By
1. Production rollout blocked on A-D gate trajectory and fault confidence.

## 8) Critical Path (Shortest Path to Real Wins)

1. **Land `E2a` lock-scope reduction** (replication + WAL encode outside engine mutex). This is the single change that structurally unblocks Gate A and Gate B. Without it, lane parallelism and batching knobs operate against a serialized lock.
2. Close `E1` with strict fault matrix (validates E2a correctness and provides production confidence).
3. Land `E11` hot-path allocation wins in parallel (compounds with E2a by shrinking the remaining critical section).
4. Run `E2/E5` knob tuning and meso sweeps against the reduced-lock baseline (now measuring real signal).
5. Use strict-only meso/macro evidence to target Gate A and Gate B trajectory.
6. Land `E3` canonical-path milestones.
7. Parallelize `E6/E7` so Gate C is not a late surprise.

## 9) 30-Day Parallel Sprint Plan

## Week 1: Lock-Scope Foundation + Allocation Quick Wins
1. **WS-LOCK**: Design and implement `prepare_and_apply_batch` split (phase 1 under lock, phase 2 replication outside lock, phase 3 commit finalization) as the default runtime path.
2. **WS-ALLOC**: Eliminate `cache_key = user_key.clone()` and `shadow_versions` triple-clone. Ship `Bytes` in `WriteEnvelope`.
3. WS-B: close remaining real-quorum gaps and failure token coverage.
4. WS-E: compaction debt metric plumbing (parallel background work).

## Week 2: Lock-Scope Validation + WAL Encode Extraction
1. **WS-LOCK**: Move WAL encoding (CRC32 + byte serialization) outside the engine lock. Strict meso comparison: lock-scope-reduced vs. baseline.
2. **WS-LOCK + WS-B**: Fault injection pass on the restructured replication call site (quorum-ACK-before-visibility, crash recovery, replay convergence).
3. **WS-ALLOC**: Memtable `get_mut` before `entry()` to avoid key clone on existing keys. Eliminate `wal_bytes.clone()` on WAL submit.
4. WS-D: canonical single-log replay contract doc + parity tests skeleton (parallel background work).

## Week 3: E2/E5 Tuning (Now Meaningful) + Memtable Evaluation
1. **WS-C**: Strict meso sweeps for `replication_batch_max_ops`, `replication_batch_max_bytes`, lane count against the reduced-lock baseline.
2. **WS-C**: Heartbeat/write coalescing + lane skew auto-balance heuristics.
3. **WS-ALLOC**: Evaluate `SmallVec` version chains and skip-list/ART memtable prototype.
4. WS-E: value-separation metadata prototype + scheduler admission scaffolding on default runtime path.

## Week 4: Evidence Package + Production Prep
1. **WS-LOCK + WS-C**: Gate A / Gate B trajectory evidence package with strict-only meso/macro runs.
2. WS-B: Phase 1 exit evidence (real-quorum + fault matrix + lock-restructure validation).
3. WS-D: shadow verification burn-in with strict runs.
4. WS-F: canary and rollback policy validation drills + operator runbook freeze v1.

## 10) Anti-Bullshit Completion Contract (Per Epic)

An epic is `done` only if all are true:
1. Code merged as default runtime behavior with coordinated-cutover rollback coverage.
2. Correctness tests added and passing.
3. Strict-run perf evidence bundle attached.
4. Off-path rollback validated.
5. Admin/telemetry surfaces shipped.
6. Runbook/docs updated.

If any item is missing, status is `incomplete`.

## 11) Reporting Format (Required Weekly)

1. What changed (code).
2. What got faster/slower (artifact links).
3. What broke.
4. Decision: continue, rollback, or kill.
5. Blockers: explicit owner + unblock date.

## 12) Immediate Next Actions (Next 7 Days)

1. **Design and implement `E2a` lock-scope split**: restructure `prepare_and_apply_batch` so replication runs outside the `DbEngine` mutex as the default behavior. This is the single highest-priority item.
2. **Ship `E11` quick wins**: eliminate `cache_key` clone, `shadow_versions` triple-clone, and `wal_bytes.clone()`. Ship `Bytes` in `WriteEnvelope`. These are small diffs with immediate critical-section reduction.
3. Continue `E1` closure: expand fault matrix to cover the restructured replication call site.
4. Capture strict baseline before E2a lands (comparison anchor for meso evidence).
5. Land compaction debt + admission scaffolding (`E6` starter) in parallel.

No strict evidence, no claim. No blockers listed, no status accepted.
