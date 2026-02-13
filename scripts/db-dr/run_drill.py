#!/usr/bin/env python3
"""Run local DR drills and emit machine-readable RPO/RTO artifacts."""

from __future__ import annotations

import argparse
import json
import pathlib
import subprocess
import sys


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run DB DR drill gate")
    parser.add_argument(
        "--out",
        default="artifacts/dr-drill-report.json",
        help="Path to drill report JSON artifact",
    )
    return parser.parse_args()


def extract_report(stdout: str) -> dict:
    marker = "DRILL_REPORT_JSON:"
    for line in stdout.splitlines():
        if line.startswith(marker):
            payload = line[len(marker) :].strip()
            return json.loads(payload)
    raise ValueError("missing DRILL_REPORT_JSON marker in test output")


def main() -> int:
    args = parse_args()
    cmd = [
        "cargo",
        "test",
        "-p",
        "wrela_runtime",
        "--test",
        "db_dr_drills",
        "--",
        "--nocapture",
    ]
    proc = subprocess.run(cmd, capture_output=True, text=True)
    sys.stdout.write(proc.stdout)
    sys.stderr.write(proc.stderr)
    if proc.returncode != 0:
        return proc.returncode

    try:
        report = extract_report(proc.stdout)
    except Exception as err:  # noqa: BLE001
        print(f"dr drill parse failed: {err}", file=sys.stderr)
        return 2

    out_path = pathlib.Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(report, indent=2), encoding="utf-8")

    if not report.get("overall_pass", False):
        print(f"dr drill gate failed; report: {out_path}")
        return 3

    print(f"dr drill gate passed; report: {out_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
