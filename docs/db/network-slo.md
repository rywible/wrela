# Network Chaos and SLO Thresholds

Deterministic local fault profiles are used to verify degraded-network behavior.

## Profiles

- `steady`
- `lossy`
- `partition-ish`

## Metrics

- `commit_p99_ms`
- `timeout_rate_pct`
- `retry_rate_pct`
- `lane_wait_gap_max` (max dispatch gap while lane has queued traffic)

## Pass Criteria

- `commit_p99_ms <= 250`
- `timeout_rate_pct <= 15.0`
- No starvation under sustained snapshot pressure:
  - `control lane_wait_gap_max <= 4 dispatches`
  - `raft lane_wait_gap_max <= 5 dispatches`

## Lane Priority and Backpressure Contract

- Scheduler dispatch weights are fixed and deterministic:
  - `control=3`, `raft=2`, `snapshot=1`, `bulk=1` per 7-slot cycle.
- Snapshot traffic is lower-priority than control/raft and must not starve either lane when all are backlogged.
- Flow-control credits (`window_bytes`) apply to all lanes; oversized head-of-line frames may block their own lane until credits return.
- The scheduler exposes state for observability/debugging:
  - `available_credits`, `in_flight_bytes`, `total_pending_frames`, `total_pending_bytes`, `schedule_cursor`
  - per-lane `pending_frames`, `pending_bytes`, `in_flight_frames`, `in_flight_bytes`, `dispatch_weight`

## Commands

```bash
cargo test -p wrela_runtime db::net::transport::tests -- --nocapture
cargo test -p wrela_runtime --test db_network_chaos -- --nocapture
python3 -m unittest discover -s scripts/db-local/net-faults/tests -p 'test_*.py'
python3 scripts/db-local/net-faults/simulate_fault_profiles.py --out artifacts/network-chaos-report.json
```
