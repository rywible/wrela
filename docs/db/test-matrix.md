# Wrela DB Test Matrix

## Phase 0/1 Required

- Keyspace encode/decode property tests.
- Limit boundary tests for key/value/batch caps.
- WAL torn-record recovery tests.
- MVCC visibility and OCC mismatch tests.
- API integration tests for put/get/range/read-after-write.
- Deterministic state-machine model tests for KV:
  - OCC success/failure transitions
  - atomic batch all-or-nothing behavior
  - read-your-write visibility across randomized operation traces
- Consensus quorum accounting tests for duplicate follower response idempotency.
- Quorum rejection isolation tests to prove rejected batches never leak into later successful writes.
- Canonical codec tests for deterministic frame round-trip and legacy-aware decode.
- Replay trace schema contract tests for sim/model failure artifacts.
- CDC cursor paging/shard-filter tests for deterministic resume behavior.
- CDC checkpoint ack monotonicity tests.
- CDC checkpoint persistence and stream-resume cursor tests.
- CDC backfill+tail and duplicate-ack storm idempotency tests.
- CDC page correctness gate tests (monotonic ordering + cursor non-regression).
- CDC perf gate threshold tests (throughput ratio, backlog cap, replay lag cap).
- gRPC edge typed error + idempotent retry semantics tests.
- gRPC idempotency token payload-mismatch rejection tests.
- gRPC client semantics validation for blank idempotency token rejection.
- Client conformance tests for `NOT_LEADER`, `RETRY_AFTER`, timeout ambiguity, and idempotency replay.
- Transport chaos classification determinism and fuzz sweep coverage.
- Network chaos integration suite for seeded fault-profile behavior and lane fairness.
- Deterministic no-starvation bound test for snapshot pressure (`control<=4`, `raft<=5` dispatch gap).
- Transport scheduler state visibility tests for per-lane pending/in-flight backpressure accounting.
- Admin surface machine-readable cluster/quorum/policy explain tests.
- Residency audit fail-closed token tests (`DENY`, `POLICY_UNSAT`).
- Residency policy canonicalization tests (`trim + lowercase`) for case/whitespace-equivalent regions.
- Autopilot replay harness deterministic scenario replay tests.
- Cost/residency compliance proof tests for guardrail decisions.
- Cost guardrail saturation tests for extreme utilization ratios without integer wraparound.
- Residency-aware read-SLO controller deterministic action tests.
- Bounded fuzz lane for DB wire/storage parsers:
  - SQL statement parser.
  - gRPC write request validation path.
  - Transport chaos classifier input decoding.
  - WAL record decode path.
  - Multipart upload session parser.
  - Schema job store canonical decode parser.
- CDC sink residency fail-closed egress tests.
  - tokenized deny path (`RESIDENCY_EGRESS_DENY`)
  - tokenized unsat-policy path (`RESIDENCY_EGRESS_POLICY_UNSAT`)
- SQL DML deterministic mutation token mapping tests.
- SQL conformance suite for parser/catalog/planner expectations.
- SQL tokenizer/parser tests for quoted values with spaces and deterministic parse failures on
  unsupported/unterminated constructs.
- Schema evolution state-machine transition legality and backfill monotonicity tests.
- Planner cost/explain deterministic selection and tie-break tests.
- Backup/restore manifest integrity and restore-plan typing tests.
- Schema job API lifecycle/persistence roundtrip tests.
- Planner stats refresh trigger + persistence codec tests.
- Snapshot manifest watermark/version validation tests.
- Raft append log-matching tests for stale-term reject, `prev_log_index/term` mismatch reject,
  conflict index reporting, conflict truncation, duplicate retry idempotency, and commit-index
  advance bounds.
- Joint membership abort rollback tests that restore outgoing voters and outgoing learners exactly.
- Safe-time observation tests proving `observe` keeps shard/region/global safe times fresh without
  manual recompute calls.
- Audit redaction tests for delimiter-aware secrets (`token:abc`, `secret=foo;`,
  `Authorization: Bearer ...`) plus JSON key redaction.
- WAL replay incremental decode tests for bounded-memory large-log replay and torn-tail truncation.
- Live-history invariant lane tests derived from real runtime DB traces (not only synthetic events).
- Multipart upload resume/retry progress contract tests.
- Reindex worker bounded/resumable stepping and remediation determinism tests.
- Plan baseline drift gate and explain-schema contract tests.
- Restore orchestration and catch-up phase planning tests.

## Required Artifacts

- `artifacts/wal-recovery.json`
- `artifacts/mvcc-visibility.json`
- `artifacts/perf-baseline.json`
- `artifacts/replay-signature-report.json`
- `artifacts/replay-ci-gate-report.json`
- `artifacts/autopilot-replay-report.json`
- `artifacts/network-chaos-report.json`
- `artifacts/sql-bench-report.json`
- `artifacts/fuzz/` (crash reproducers and minimized inputs)

## Artifact Commands

- `bash scripts/db-chaos/smoke.sh`
- `bash scripts/db-chaos/fuzz-smoke.sh`
- `bash scripts/db-bench/baseline.sh`
- `cargo test -p wrela_runtime --test db_model_kv -- --nocapture`
- `cargo test -p wrela_runtime --test db_consensus_faults -- --nocapture`
- `cargo test -p wrela_runtime db::codec::tests -- --nocapture`
- `cargo test -p wrela_runtime db::raft::append::tests -- --nocapture`
- `cargo test -p wrela_runtime db::replication:: -- --nocapture`
- `cargo test -p wrela_runtime db::time::safe_time::tests -- --nocapture`
- `cargo test -p wrela_runtime db::audit::tests -- --nocapture`
- `cargo test -p wrela_runtime db::wal::segment::tests -- --nocapture`
- `cargo test -p wrela_runtime --test db_invariant_history -- --nocapture`
- `bash scripts/db-jepsen/check_history.sh`
- `cargo test -p wrela_runtime cdc_page_ -- --nocapture`
- `cargo test -p wrela_runtime cdc_checkpoint_persists_across_restart -- --nocapture`
- `cargo test -p wrela_runtime cdc_stream_page_resumes_from_stored_checkpoint -- --nocapture`
- `cargo test -p wrela_runtime cdc_stream_backfill_then_tail_uses_checkpoint_after_first_ack -- --nocapture`
- `cargo test -p wrela_runtime cdc_duplicate_ack_storm_is_idempotent_across_restart -- --nocapture`
- `cargo test -p wrela_runtime cdc_correctness_gate_ -- --nocapture`
- `cargo test -p wrela_runtime cdc_perf_gate_ -- --nocapture`
- `cargo test -p wrela_runtime poll_cdc_page_meets_correctness_gate -- --nocapture`
- `cargo test -p wrela_runtime bootstrap_metadata_authority_ -- --nocapture`
- `cargo test -p wrela_runtime failover_metadata_authority_ -- --nocapture`
- `cargo test -p wrela_runtime rebootstrap_metadata_authority_ -- --nocapture`
- `cargo test -p wrela_runtime rpc::errors::tests -- --nocapture`
- `cargo test -p wrela_runtime rpc::grpc::tests -- --nocapture`
- `cargo test -p wrela_runtime --test db_client_conformance -- --nocapture`
- `cargo test -p wrela_runtime db::net::transport::tests -- --nocapture`
- `cargo test -p wrela_runtime --test db_network_chaos -- --nocapture`
- `cargo test -p wrela_runtime --test db_admin_surface -- --nocapture`
- `cargo test -p wrela_runtime --test db_autopilot_replay -- --nocapture`
- `cargo test -p wrela_runtime --test db_policy_compiler -- --nocapture`
- `cargo test -p wrela_runtime compliance::tests -- --nocapture`
- `cargo test -p wrela_runtime read_slo_controller::tests -- --nocapture`
- `cargo test -p wrela_runtime poll_cdc_for_sink_ -- --nocapture`
- `cargo test -p wrela_runtime db::security::residency::tests -- --nocapture`
- `cargo test -p wrela_runtime db::sql::tests -- --nocapture`
- `cargo test -p wrela_runtime --test db_sql_conformance -- --nocapture`
- `cargo test -p wrela_runtime schema_evolution::tests -- --nocapture`
- `cargo test -p wrela_runtime planner::tests -- --nocapture`
- `cargo test -p wrela_runtime backup::tests -- --nocapture`
- `cargo test -p wrela_runtime schema_evolution::tests -- --nocapture`
- `cargo test -p wrela_runtime planner::tests -- --nocapture`
- `cargo test -p wrela_runtime snapshot:: -- --nocapture`
- `cargo test -p wrela_runtime restore::tests -- --nocapture`
- `cargo test -p wrela --test cli cli_test_sim_lane_seed_filter_and_trace_artifact -- --nocapture`
- `cargo test -p wrela --test cli cli_test_model_lane_seed_filter_and_artifact -- --nocapture`
- `python3 -m unittest discover -s scripts/db-chaos/tests -p 'test_*.py'`
- `python3 -m unittest discover -s scripts/db-local/net-faults/tests -p 'test_*.py'`
- `python3 scripts/db-local/net-faults/simulate_fault_profiles.py --out artifacts/network-chaos-report.json`
- `python3 -m unittest discover -s scripts/db-autopilot-sim/tests -p 'test_*.py'`
- `python3 scripts/db-autopilot-sim/run.py --input scripts/db-autopilot-sim/example-input.json --out artifacts/autopilot-replay-report.json`
- `python3 scripts/db-bench/sql/run_sql_bench.py --out artifacts/sql-bench-report.json`
- `scripts/db-chaos/replay_ci_gate.py --canonical-root docs/db/replay-corpus/v1 --candidate-root tests/.artifacts --baseline-perf artifacts/perf-baseline-main.json --candidate-perf artifacts/perf-baseline.json --out artifacts/replay-ci-gate-report.json`

## Gate Expectations

- Replay CI gate pass stdout: `replay CI gate passed; report: artifacts/replay-ci-gate-report.json`
- Replay CI gate fail stdout: `replay CI gate failed; report: artifacts/replay-ci-gate-report.json`
- Replay CI gate must fail on placeholder corpus manifests (`determinism.empty_manifest`) and invalid manifest schema (`determinism.invalid_manifest_schema`).
