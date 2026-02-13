# Schema Evolution Seed (WRE-598)

This seed adds deterministic schema-job APIs and orchestration persistence in:

- `runtime/src/db/schema_evolution/mod.rs`

## Contract

### Typed schema job API

`SchemaJobStore` now exposes typed orchestration APIs:

- `create_job(request) -> SchemaJobId`
  - Deterministic ID derived from canonical request bytes (`sha256` over JSON bytes).
  - Idempotent for identical requests.
- `status(job_id) -> Option<SchemaJobStatus>`
  - Stable external status projection (`Draft`, `Backfill`, `Validate`, `Cutover`, `Complete`, `Canceled`, `RolledBack`).
- `cancel_job(job_id) -> Result<(), JobError>`
  - Legal from `Draft|Backfill|Validate|Cutover` only.
- `rollback_job(job_id) -> Result<(), JobError>`
  - Legal from `Validate|Cutover` only.

Transition violations return typed errors:

- `JobError::IllegalCancel { status }`
- `JobError::IllegalRollback { status }`
- `JobError::IllegalTransition { from, to }`
- `JobError::JobNotFound { job_id }`

### Deterministic lifecycle events

Each job tracks append-only deterministic lifecycle events with monotonic sequence numbers:

- `Created`
- `PhaseChanged { from, to }`
- `Canceled`
- `RolledBack`

Events are queryable via `events(job_id)` and are persisted in canonical state bytes.

### Canonical state persistence

`SchemaJobStore` can serialize/deserialize orchestration state:

- `to_canonical_bytes() -> Result<Vec<u8>, PersistenceError>`
- `from_canonical_bytes(bytes) -> Result<SchemaJobStore, PersistenceError>`

Determinism guarantees:

- jobs are encoded in sorted `SchemaJobId` order (`BTreeMap` + canonical entry vector)
- event order is preserved by sequence
- roundtrip bytes are stable (`serialize -> deserialize -> serialize`)

## Test coverage

`schema_evolution::tests` includes:

- existing phase machine and backfill progress invariants
- deterministic job ID + idempotent create behavior
- crash-resume roundtrip with canonical byte stability
- ordering determinism independent of job insertion order
- legal/illegal cancel transitions
- legal/illegal rollback transitions
- stable status reporting across lifecycle phases

## Verification

```bash
cargo test -p wrela_runtime schema_evolution::tests -- --nocapture
```

---

# Schema Evolution Seed (WRE-599)

This update adds deterministic reindex worker semantics, validation mismatch planning, and
cutover readiness gating in:

- `runtime/src/db/schema_evolution/mod.rs`

## Contract

### Reindex worker model (bounded + resumable)

Typed APIs:

- `ReindexWorkerConfig { max_batch_rows, max_in_flight }`
- `ReindexWorkerState { progress_cursor, in_flight_rows, resume_token }`
- `ReindexWorker::step(state, total_rows) -> ReindexStep`

Behavior guarantees:

- `step` never assigns more than `max_batch_rows`.
- `step` never causes `in_flight_rows` to exceed `max_in_flight`.
- `progress_cursor` is monotonic and bounded by `total_rows`.
- state can resume deterministically via `ResumeToken` roundtrip.

### Validation mismatch classification + remediation hooks

Typed mismatch model:

- `ValidationMismatch::MissingRow`
- `ValidationMismatch::DivergentValue`
- `ValidationMismatch::ExtraIndexEntry`
- `ValidationMismatch::MissingIndexEntry`

Classification/remediation hooks:

- `classify_mismatch(observation) -> ValidationMismatch`
- `plan_remediation_actions(mismatches) -> Vec<RemediationAction>`

Determinism guarantees:

- remediation plans are sorted and deduplicated
- identical mismatch sets produce identical action lists independent of input ordering

### Cutover readiness gate

Typed API:

- `evaluate_cutover_readiness(input) -> CutoverReadiness`

When not ready, typed reasons include:

- `BackfillIncomplete`
- `ReindexWorkInFlight`
- `ValidationMismatchesPending`
- `RemediationPending`

Reasons are returned in deterministic priority order.

## Test coverage

`schema_evolution::tests` now includes:

- bounded/resumable reindex stepping
- deterministic mismatch classification/remediation planning
- cutover readiness gate behavior for both `Ready` and `NotReady` states

## Verification

```bash
cargo test -p wrela_runtime schema_evolution::tests -- --nocapture
```
