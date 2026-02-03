#!/usr/bin/env python3
import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path

DEFAULT_BASELINE = Path("bench/baselines/perf.json")

PERF_RE = re.compile(r"^perf:\s+p50_ns=(\d+)\s+p99_ns=(\d+)\s+allocs/request=([0-9.]+)\s*$")


def parse_perf_line(output: str):
    for line in output.splitlines():
        match = PERF_RE.match(line.strip())
        if match:
            return {
                "p50_ns": int(match.group(1)),
                "p99_ns": int(match.group(2)),
                "allocs_per_request": float(match.group(3)),
            }
    return None


def write_test_project(root: Path):
    src_dir = root / "src"
    tests_dir = root / "tests"
    src_dir.mkdir(parents=True, exist_ok=True)
    tests_dir.mkdir(parents=True, exist_ok=True)
    (src_dir / "main.wr").write_text("to run() -> Integer:\n    return 0\n")
    (tests_dir / "basic.wr").write_text(
        "to test_basic() -> Nothing:\n    assert value 1 == 1\n"
    )


def run_perf_sample(root: Path):
    cmd = ["cargo", "run", "-p", "wrela", "--", "test", str(root)]
    result = subprocess.run(cmd, capture_output=True, text=True)
    stdout = result.stdout
    stderr = result.stderr
    if result.returncode != 0:
        sys.stderr.write(stdout)
        sys.stderr.write(stderr)
        raise SystemExit(f"perf run failed with exit code {result.returncode}")
    perf = parse_perf_line(stdout)
    if perf is None:
        sys.stderr.write(stdout)
        sys.stderr.write(stderr)
        raise SystemExit("perf summary line not found in output")
    return perf, stdout


def run_perf_samples(runs: int):
    with tempfile.TemporaryDirectory(prefix="wrela_perf_") as tmp:
        root = Path(tmp)
        write_test_project(root)
        best_perf = None
        best_stdout = ""
        for _ in range(runs):
            perf, stdout = run_perf_sample(root)
            if best_perf is None or perf["p99_ns"] < best_perf["p99_ns"]:
                best_perf = perf
                best_stdout = stdout
        return best_perf, best_stdout


def load_baseline(path: Path):
    if not path.exists():
        return {"version": 1, "targets": {}}
    return json.loads(path.read_text())


def save_baseline(path: Path, baseline):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(baseline, indent=2, sort_keys=True) + "\n")


def calc_limit(base_p99: int, threshold: float) -> int:
    return int(base_p99 * (1.0 + threshold))


def main():
    parser = argparse.ArgumentParser(description="Perf regression gate for wrela.")
    parser.add_argument("--baseline", default=str(DEFAULT_BASELINE))
    parser.add_argument("--threshold", type=float, default=0.05)
    args = parser.parse_args()

    perf_key = os.environ.get("WRELA_PERF_KEY", "default")
    allow_regression = os.environ.get("WRELA_PERF_ALLOW_REGRESSION") == "1"
    update_baseline = os.environ.get("WRELA_PERF_UPDATE") == "1"

    baseline_path = Path(args.baseline)
    baseline = load_baseline(baseline_path)
    targets = baseline.setdefault("targets", {})

    runs = int(os.environ.get("WRELA_PERF_RUNS", "3"))
    perf, stdout = run_perf_samples(max(1, runs))

    current = {
        **perf,
        "updated_at": datetime.now(timezone.utc).strftime("%Y-%m-%d"),
    }

    if update_baseline:
        targets[perf_key] = current
        save_baseline(baseline_path, baseline)
        print(f"perf gate: baseline updated for {perf_key}")
        print(stdout)
        return 0

    if perf_key not in targets:
        raise SystemExit(
            f"perf gate: missing baseline for {perf_key}. "
            f"Set WRELA_PERF_UPDATE=1 to write a baseline."
        )

    base = targets[perf_key]
    limit = calc_limit(base["p99_ns"], args.threshold)
    if perf["p99_ns"] > limit and not allow_regression:
        raise SystemExit(
            "perf gate: p99 regression detected. "
            f"baseline={base['p99_ns']} current={perf['p99_ns']} limit={limit}."
        )

    print(
        "perf gate: ok "
        f"(baseline p99={base['p99_ns']} current p99={perf['p99_ns']} limit={limit})"
    )
    if allow_regression:
        print("perf gate: regression allowed by WRELA_PERF_ALLOW_REGRESSION=1")
    print(stdout)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
