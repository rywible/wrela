# SQL/Policy Prereq Seed (WRE-489, WRE-497)

This seed introduces minimal-but-real runtime behavior for policy prerequisites:

- `runtime/src/db/routing/mod.rs`
  - Composite shard-key policy enforcement.
  - Single-field shard keys require an explicit non-empty waiver reason.
  - Deterministic shard route selection from serialized shard-key components.
- `runtime/src/db/autopilot/mod.rs`
  - Shard-skew preflight gate: `max(shard_load)/mean(shard_load) <= threshold`.
  - Default threshold exported as `DEFAULT_MAX_SKEW_RATIO = 1.5`.
- `runtime/src/db/mod.rs`
  - Public transaction lock wrappers: `txn_lock_key(handle, txn_id, namespace, key)` and
    `txn_lock_range(handle, txn_id, namespace, start_key, end_key)` to bind SQL DML to txn lock
    paths outside the engine internals.
- `runtime/src/db/sql/mod.rs`
  - Deterministic SQL DML seed planner/executor modeling row mutations plus secondary-index
    maintenance.
  - Lock acquisition remains deterministic (sorted), while mutation apply order follows caller order
    so same-key `Put/Delete` sequencing is preserved.
  - Minimal SQL statement layer for phase conformance:
    - parser (`INSERT`, `DELETE`, `EXPLAIN`) with deterministic typed `InvalidArgument` failures.
    - deterministic tokenization supports quoted tokens (single or double quotes) for values
      containing whitespace.
    - unterminated quoted tokens fail with typed parse errors.
    - catalog validation for table/index existence before mutation planning.
    - planner explain binding via `compile_statement(...) -> CompiledSql::Explain(...)`.
  - Conformance harness:
    - `ConformanceCase` + `run_conformance_suite` executes parser/catalog/planner checks and
      returns deterministic pass/fail records per case.
  - Transactional execute flow: begin txn, key locks, `submit_batch`, prepare+commit on success,
    abort on error.
  - Read integration follows DB strong-read defaults; conformance tests can explicitly request
    eventual consistency where appropriate.
  - `execute_with_result(handle, mutations)` adds a deterministic SQL mutation error surface for
    callers while preserving `execute(handle, mutations) -> Result<u64, DbError>`.
  - Deterministic SQL mutation tokens:
    - `SQL_RETRYABLE_CONFLICT`: lock conflict/deadlock-victim errors mapped from
      `DbError { code: LimitExceeded, ... }`.
    - `SQL_INVALID_MUTATION`: invalid SQL DML mutation inputs (for example, empty mutation list)
      mapped from `DbError { code: InvalidArgument, ... }`.
  - Module tests covering:
    - put/index writes, delete row/index cleanup, and conflict lock failure with no partial writes.
    - deterministic SQL mutation token mapping assertions.
    - parser/catalog/explain compile behavior.
    - conformance harness deterministic pass/fail reporting.

## Verification

```bash
cargo test -p wrela_runtime --test db_routing_policy
cargo test -p wrela_runtime --test db_autopilot_skew
cargo test -p wrela_runtime sql:: -- --nocapture
cargo test -p wrela_runtime db::sql::tests -- --nocapture
cargo test -p wrela_runtime --lib
```
