# Replay Trace Schema (Phase 10 Seed)

## Artifact Location

- Sim lane failures: `tests/.artifacts/sim/<canonical-test-id>/<seed>.json`
- Model lane failures: `tests/.artifacts/model/<canonical-test-id>/<seed>.json`
- Canonical replay corpus: `docs/db/replay-corpus/v1`

## Contract

Schema version: `1`

Top-level fields:

- `version`: schema version
- `generated_at_unix_ms`: wall-clock generation timestamp
- `test_id`: concrete test id
- `canonical_test_id`: stable canonical id
- `lane`: replay lane (`sim` or `model` for current seed)
- `seed`: deterministic seed used for run
- `failure`: failure message summary
- `events`: ordered event envelope list

Event envelope fields:

- `seq`: monotonic event order index
- `operation`:
  - `phase`
  - `action`
  - `commit_state`
- `route`:
  - `lane`
  - `scheduler_seed`
  - `target`
- `timing`:
  - `logical_step`
  - `observed_unix_ms`
- `fault` (optional):
  - `kind`
  - `source`
  - `seed`
  - `detail`
- `outcome`

## Determinism Requirements

- Serialization order is stable for identical input payloads.
- `events` ordering is deterministic by `seq`.
- `seed` is copied into both route and fault metadata for replay portability.

## Validation

- Unit contract: `cargo test -p wrela --bin wrela replay_trace::tests -- --nocapture`
- Integration contract:
  - `cargo test -p wrela --test cli cli_test_sim_lane_seed_filter_and_trace_artifact -- --nocapture`
  - `cargo test -p wrela --test cli cli_test_model_lane_seed_filter_and_artifact -- --nocapture`
- Artifact verifier:
  - `scripts/db-chaos/verify_replay_trace.py [tests/.artifacts]`
- CLI deterministic replay signature check:
  - `wrela test --replay-trace <artifact.json> <project-root>`
- Cross-run deterministic replay gate (CI):
  - `scripts/db-chaos/compare_replay_signatures.py --baseline-root <baseline-artifacts> --candidate-root <candidate-artifacts> --out artifacts/replay-signature-report.json`
- Full replay CI gate (invariants + determinism + perf):
  - `scripts/db-chaos/replay_ci_gate.py --canonical-root docs/db/replay-corpus/v1 --candidate-root tests/.artifacts --baseline-perf <main-perf.json> --candidate-perf <candidate-perf.json> --out artifacts/replay-ci-gate-report.json`
