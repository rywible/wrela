# Scrub and Repair Playbook

## Targets

- WAL segments
- SSTables
- Snapshot artifacts

## Detection Contract

Scrub reports checksum mismatches with severity:

- `Warning` for WAL/SST mismatches
- `Critical` for snapshot mismatches
- Scrub defers when foreground latency budget is exceeded (`baseline_p99 -> observed_p99` over configured max delta).

## Repair Actions

- WAL/SST mismatch: `RebuildFollower`
- Snapshot mismatch: `RefetchSnapshot`
- Any critical finding: quarantine required
- Reports carry deterministic `trace_id` so repair actions are auditable across retries.

## Verification

```bash
cargo test -p wrela_runtime db::scrub::
cargo test -p wrela_runtime --test db_scrub_repair_drill
```
