# WRE-494 Restore Pipeline Seed

This seed introduces a typed model for restore load and raft catch-up planning in
`runtime/src/db/restore/mod.rs`.

## Restore Load Request

`RestoreLoadRequest` defines the pre-activation input:

- `source_uri`: backup/snapshot location.
- `expected_snapshot`: expected `last_index`, `last_term`, and `checksum`.
- `snapshot_manifest`: typed snapshot manifest.
- `snapshot_payload`: payload bytes used for checksum validation.

`validate_restore_load_request` performs pre-activation checks by composing existing validation
primitives:

1. `verify_snapshot_manifest_consistency` (manifest consistency against expected metadata).
2. `SnapshotManifest::validate_payload` (checksum and manifest semantics).

## Raft Catch-Up Plan Model

`CatchUpPhase` is explicitly typed:

- `InstallSnapshot { snapshot_index, snapshot_term }`
- `ReplayTail { start_index, end_index }`
- `Steady`

`plan_catch_up_phases` is deterministic from index inputs:

- follower behind snapshot boundary -> install snapshot, optionally replay tail, then steady.
- follower within leader log span -> replay tail then steady.
- follower at leader tip -> steady.
- follower ahead of leader -> typed planner error.

## Orchestration

`orchestrate_restore` performs:

1. status transition to `Validating`.
2. pre-activation validation.
3. deterministic catch-up planning.
4. status transitions derived from catch-up phases.

Typed result:

- `RestoreOutcome { validated, catch_up_plan, transitions, final_status }`

Errors are typed through `RestoreOrchestrationError`:

- `Validation(RestoreValidationError)`
- `CatchUp(CatchUpPlannerError)`

## Tests

Module tests cover:

- success path (validation + install snapshot + replay tail + steady transitions)
- invalid manifest path (pre-activation checksum mismatch)
- deterministic catch-up phase transitions
