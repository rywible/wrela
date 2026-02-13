#!/usr/bin/env bash
set -euo pipefail

ARTIFACT_DIR="${ARTIFACT_DIR:-artifacts}"
RUNS="${RUNS:-15}"
mkdir -p "$ARTIFACT_DIR"

python3 - "$ARTIFACT_DIR" "$RUNS" <<'PY'
import json
import pathlib
import statistics
import subprocess
import sys
import time

artifact_dir = pathlib.Path(sys.argv[1])
runs = int(sys.argv[2])

test_cmd = [
    "cargo",
    "test",
    "-p",
    "wrela_runtime",
    "tests::db_abi_put_get_scan_roundtrip",
    "--",
    "--exact",
]

latencies_ns = []
for _ in range(runs):
    t0 = time.perf_counter_ns()
    proc = subprocess.run(test_cmd, check=True, capture_output=True, text=True)
    if "test result: ok. 1 passed" not in proc.stdout:
        raise RuntimeError("expected exactly one matching test to run")
    latencies_ns.append(time.perf_counter_ns() - t0)

latencies_ns.sort()

def percentile_ns(samples: list[int], pct: float) -> int:
    if not samples:
        return 0
    rank = round((len(samples) - 1) * pct)
    return samples[max(0, min(rank, len(samples) - 1))]

total_ns = sum(latencies_ns)
total_s = total_ns / 1_000_000_000.0
ops_per_run = 3  # put + point read + range scan
ops_total = runs * ops_per_run
ops_per_sec = (ops_total / total_s) if total_s > 0 else 0.0

p50_ms = percentile_ns(latencies_ns, 0.50) / 1_000_000.0
p95_ms = percentile_ns(latencies_ns, 0.95) / 1_000_000.0
p99_ms = percentile_ns(latencies_ns, 0.99) / 1_000_000.0

payload = {
    "lane": "single-node-mixed-rw",
    "ops_per_sec": ops_per_sec,
    "p95_ms": p95_ms,
    "p99_ms": p99_ms,
    "batch_size": 1,
    "sample_runs": runs,
    "wal_fsync_summary": {
        "p50_ms": p50_ms,
        "p95_ms": p95_ms,
        "p99_ms": p99_ms,
        "method": "measured via db_abi_put_get_scan_roundtrip runtime test",
    },
    "timing_summary": {
        "min_ms": min(latencies_ns) / 1_000_000.0,
        "max_ms": max(latencies_ns) / 1_000_000.0,
        "avg_ms": statistics.fmean(latencies_ns) / 1_000_000.0,
    },
    "generated_at_unix_ms": int(time.time() * 1000),
}

(artifact_dir / "perf-baseline.json").write_text(
    json.dumps(payload, indent=2) + "\n",
    encoding="utf-8",
)
PY

echo "baseline artifact emitted to $ARTIFACT_DIR/perf-baseline.json"
