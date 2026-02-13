# CDC Ordering Contract (Phase 11 Seed)

## Source-of-Truth Emission Point

- CDC events are emitted only from committed apply path (`DbEngine::submit_batch`).
- Aborted transaction paths must not emit CDC records.

## Event Shape

- `commit_seq`: monotonic commit sequence number.
- `shard`: shard identifier bytes (current seed uses namespace bytes).
- `key`: user key bytes.
- `kind`: `Put` or `Delete`.
- `value`: optional value payload (`None` for deletes).
- `version`: commit version from HLC tick at apply.

## Invariants

- `commit_seq` must be strictly increasing.
- Event order matches committed apply order.
- CDC stream excludes aborted transaction intents.
- Polling supports bounded pages and stable resume cursor semantics:
  - `next_commit_seq` equals last emitted event sequence for non-empty pages.
  - empty page keeps caller cursor unchanged.
  - `high_watermark` exposes current emitter tip for lag accounting.
- Optional shard filter must preserve commit ordering for matching events.
- Stream checkpoint acks are monotonic per stream:
  - `ack(stream, seq)` cannot move checkpoint backwards.
  - checkpoints are isolated by stream id.
- Checkpoints survive process restart (persisted under DB data directory).
- Stream polling may resume from stored stream checkpoint (`stream page` helper).
- Backfill+tail semantics:
  - If stream has no checkpoint, start from `backfill_start_inclusive`.
  - Once checkpoint exists, checkpoint cursor takes precedence over backfill start.
- Duplicate ack storms must be idempotent before and after restart.
- Correctness gate for API pages:
  - page events must remain strictly monotonic by `commit_seq`.
  - `next_commit_seq` must never regress below caller cursor.
- Perf gate for X1 bootstrap/rebootstrap readiness:
  - sink/source throughput ratio must be >= configured floor.
  - backlog events must remain <= configured cap.
  - replay lag seconds must remain <= configured cap.

## Perf Gate Defaults (Seed)

- `min_sink_to_source_ratio = 0.90`
- `max_backlog_events = 50_000`
- `max_replay_lag_seconds = 120`

## Runtime Tests

- `cargo test -p wrela_runtime cdc_emits_committed_apply_order_with_stable_commit_sequence -- --nocapture`
- `cargo test -p wrela_runtime cdc_never_emits_for_aborted_transactions -- --nocapture`
- `cargo test -p wrela_runtime cdc_page_paginates_with_resume_cursor -- --nocapture`
- `cargo test -p wrela_runtime cdc_page_honors_shard_filter_and_keeps_monotonic_cursor -- --nocapture`
- `cargo test -p wrela_runtime poll_cdc_authorized_supports_resume_cursor -- --nocapture`
- `cargo test -p wrela_runtime cdc_ack_checkpoint_is_monotonic_and_stream_scoped -- --nocapture`
- `cargo test -p wrela_runtime cdc_ack_authorized_enforces_monotonic_checkpoint -- --nocapture`
- `cargo test -p wrela_runtime cdc_checkpoint_persists_across_restart -- --nocapture`
- `cargo test -p wrela_runtime cdc_stream_page_resumes_from_stored_checkpoint -- --nocapture`
- `cargo test -p wrela_runtime poll_cdc_stream_uses_checkpoint_cursor -- --nocapture`
- `cargo test -p wrela_runtime cdc_stream_backfill_then_tail_uses_checkpoint_after_first_ack -- --nocapture`
- `cargo test -p wrela_runtime cdc_duplicate_ack_storm_is_idempotent_across_restart -- --nocapture`
- `cargo test -p wrela_runtime poll_cdc_backfill_then_tail_prefers_checkpoint_after_ack -- --nocapture`
- `cargo test -p wrela_runtime cdc_correctness_gate_accepts_monotonic_page -- --nocapture`
- `cargo test -p wrela_runtime cdc_correctness_gate_rejects_non_monotonic_page -- --nocapture`
- `cargo test -p wrela_runtime cdc_perf_gate_fails_on_throughput_backlog_and_replay_lag -- --nocapture`
- `cargo test -p wrela_runtime cdc_perf_gate_passes_when_thresholds_hold -- --nocapture`
- `cargo test -p wrela_runtime poll_cdc_page_meets_correctness_gate -- --nocapture`
