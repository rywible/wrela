#!/usr/bin/env bash
set -euo pipefail

ARTIFACT_DIR="${ARTIFACT_DIR:-artifacts}"
mkdir -p "$ARTIFACT_DIR"

run_test() {
  local test_name="$1"
  local started_ns ended_ns duration_ns status log_file

  log_file="$(mktemp)"
  trap 'rm -f "$log_file"' RETURN

  started_ns="$(python3 - <<'PY'
import time
print(time.perf_counter_ns())
PY
)"

  if cargo test -p wrela_runtime "$test_name" -- --exact 2>&1 | tee "$log_file" >/dev/null && rg -q "test result: ok. 1 passed" "$log_file"; then
    status=pass
  else
    status=fail
  fi

  ended_ns="$(python3 - <<'PY'
import time
print(time.perf_counter_ns())
PY
)"

  duration_ns=$((ended_ns - started_ns))
  echo "$status:$duration_ns"
}

wal_result="$(run_test db::tests::wal_recovery_replays_data)"
mvcc_result="$(run_test db::tests::namespace_isolation_and_occ_mismatch_are_deterministic)"

wal_status="${wal_result%%:*}"
wal_duration_ns="${wal_result##*:}"
mvcc_status="${mvcc_result%%:*}"
mvcc_duration_ns="${mvcc_result##*:}"

python3 - "$ARTIFACT_DIR" "$wal_status" "$wal_duration_ns" "$mvcc_status" "$mvcc_duration_ns" <<'PY'
import json
import pathlib
import sys
import time

artifact_dir = pathlib.Path(sys.argv[1])
wal_status = sys.argv[2]
wal_duration_ns = int(sys.argv[3])
mvcc_status = sys.argv[4]
mvcc_duration_ns = int(sys.argv[5])

wal_payload = {
    "scenario": "single-node-reopen-recovery",
    "status": wal_status,
    "recovered": wal_status == "pass",
    "duration_ms": wal_duration_ns / 1_000_000.0,
    "generated_at_unix_ms": int(time.time() * 1000),
}
(artifact_dir / "wal-recovery.json").write_text(
    json.dumps(wal_payload, indent=2) + "\n",
    encoding="utf-8",
)

mvcc_payload = {
    "scenario": "mvcc-occ-namespace-visibility",
    "status": mvcc_status,
    "stale_expected_version_rejected": mvcc_status == "pass",
    "cross_namespace_isolation_ok": mvcc_status == "pass",
    "duration_ms": mvcc_duration_ns / 1_000_000.0,
    "generated_at_unix_ms": int(time.time() * 1000),
}
(artifact_dir / "mvcc-visibility.json").write_text(
    json.dumps(mvcc_payload, indent=2) + "\n",
    encoding="utf-8",
)

ack_write_loss = not (wal_status == "pass")
occ_guard = mvcc_status == "pass"
namespace_isolation = mvcc_status == "pass"
status = "pass" if (not ack_write_loss and occ_guard and namespace_isolation) else "fail"

payload = {
    "scenario": "single-node-kill-recover",
    "status": status,
    "invariants": {
        "ack_write_loss": ack_write_loss,
        "occ_guard": occ_guard,
        "namespace_isolation": namespace_isolation,
    },
}
(artifact_dir / "chaos-smoke.json").write_text(
    json.dumps(payload, indent=2) + "\n",
    encoding="utf-8",
)
PY

echo "chaos smoke artifact emitted to $ARTIFACT_DIR/chaos-smoke.json"
